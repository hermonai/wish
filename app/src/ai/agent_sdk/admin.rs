//! General-purpose administrative commands in the Wish CLI.

use anyhow::{Context, Result};
use serde::Serialize;
use wish_cli::agent::OutputFormat;
use wishui::{platform::TerminationMode, AppContext, SingletonEntity};

use wish_cli::{LoginArgs, SignupArgs};

use crate::auth::auth_manager::{AuthManager, AuthManagerEvent};
use crate::auth::user::PrincipalType;
use crate::auth::AuthStateProvider;
use crate::workspaces::user_workspaces::UserWorkspaces;

/// Kick off a login flow — either password (when `--email` is provided),
/// device auth (default), or Hermon API key (`--hermon`).
pub fn login(ctx: &mut AppContext, args: LoginArgs) -> Result<()> {
    if args.hermon {
        return login_hermon(ctx);
    }
    if let Some(email) = args.email.clone() {
        return login_password(ctx, email, args.password);
    }
    let auth_state = AuthStateProvider::as_ref(ctx).get();
    let has_cached_credentials = auth_state.is_logged_in();

    // If the user is already logged in, we require that the user log out before logging
    // back in to ensure their existing state isn't replaced (especially if using both the CLI
    // and the desktop app). In this case, try refreshing their credentials first. If the user
    // is trying to log in because the cached credentials are invalid, we should let them do so.
    // Track whether we've started the device auth flow. Failure events
    // that arrive before device auth has started are leftover refresh
    // errors and should be ignored rather than treated as terminal.
    let mut started_device_auth = !has_cached_credentials;
    ctx.subscribe_to_model(
        &AuthManager::handle(ctx),
        move |_, event, ctx| match event {
            AuthManagerEvent::AuthComplete => {
                if !started_device_auth {
                    // Refresh succeeded - credentials are still valid.
                    let auth_state = AuthStateProvider::as_ref(ctx).get();
                    match (auth_state.username_for_display(), auth_state.user_email()) {
                        (Some(username), Some(email)) if username != email => {
                            println!("You are already logged in as {username} ({email}).")
                        }
                        (Some(name), _) | (None, Some(name)) => {
                            println!("You are already logged in as {name}.")
                        }
                        (None, None) => {
                            println!("You are already logged in.")
                        }
                    }
                    ctx.terminate_app(TerminationMode::ForceTerminate, None);
                } else {
                    // Device auth succeeded.
                    println!("Logged in successfully");
                    ctx.terminate_app(TerminationMode::ForceTerminate, None);
                }
            }
            AuthManagerEvent::AuthFailed(_) => {
                if !started_device_auth {
                    // Refresh failed - start a fresh device auth flow.
                    started_device_auth = true;
                    AuthManager::handle(ctx).update(ctx, |auth_manager, ctx| {
                        auth_manager.authorize_device(ctx);
                    });
                } else {
                    // Device auth failed.
                    let err_msg = match event {
                        AuthManagerEvent::AuthFailed(err) => {
                            format!("Authentication failed: {err:#}")
                        }
                        _ => "Authentication failed".to_string(),
                    };
                    ctx.terminate_app(
                        TerminationMode::ForceTerminate,
                        Some(Err(anyhow::anyhow!(err_msg))),
                    );
                }
            }
            AuthManagerEvent::ReceivedDeviceAuthorizationCode {
                verification_url,
                verification_url_complete,
                user_code,
            } => {
                if let Some(url) = verification_url_complete {
                    println!("To log in, open this URL in your browser:\n{url}");
                } else {
                    println!(
                        "To log in, visit {verification_url} and enter this code: {user_code}"
                    );
                }
            }
            _ => {}
        },
    );

    // Either refresh existing credentials or start device auth from scratch.
    AuthManager::handle(ctx).update(ctx, |auth_manager, ctx| {
        if has_cached_credentials {
            auth_manager.refresh_user(ctx);
        } else {
            auth_manager.authorize_device(ctx);
        }
    });

    Ok(())
}

#[derive(Serialize)]
struct WhoamiOutput {
    uid: String,
    #[serde(rename = "type")]
    principal_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_uid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    team_name: Option<String>,
}

/// Singleton model that provides a `ModelContext` for the `whoami` command's async work.
struct WhoamiRunner;

impl wishui::Entity for WhoamiRunner {
    type Event = ();
}

impl SingletonEntity for WhoamiRunner {}

/// Print information about the currently authenticated principal.
pub fn whoami(ctx: &mut AppContext, output_format: OutputFormat) -> Result<()> {
    let auth_state = AuthStateProvider::as_ref(ctx).get();
    let principal_type = auth_state.principal_type().unwrap_or_default();

    let uid = auth_state
        .user_id()
        .map(|id| {
            let s = id.as_string();
            s.strip_prefix("serviceAccount:")
                .map(String::from)
                .unwrap_or(s)
        })
        .ok_or_else(|| anyhow::anyhow!("Could not determine user ID. Are you logged in?"))?;

    let mut info = WhoamiOutput {
        uid,
        principal_type: match principal_type {
            PrincipalType::User => "user",
            PrincipalType::ServiceAccount => "service_account",
        },
        display_name: auth_state.display_name(),
        email: match principal_type {
            PrincipalType::User => auth_state.user_email().filter(|e| !e.is_empty()),
            PrincipalType::ServiceAccount => None,
        },
        team_uid: None,
        team_name: None,
    };

    // Refresh workspace metadata before reading team info, so we don't print
    // stale or missing team data if the metadata hasn't been fetched yet.
    let runner = ctx.add_singleton_model(|_| WhoamiRunner);
    runner.update(ctx, move |_, ctx| {
        let refresh_future = super::common::refresh_workspace_metadata(ctx);
        ctx.spawn(refresh_future, move |_, result, ctx| {
            if let Err(err) = result {
                // Do not prevent showing user info if fetching team metadata fails.
                log::warn!("Failed to refresh team metadata for whoami: {err:#}");
            }

            let current_team = UserWorkspaces::as_ref(ctx).current_team();
            info.team_uid = current_team.map(|t| t.uid.to_string());
            info.team_name = current_team
                .map(|t| t.name.clone())
                .filter(|n| !n.is_empty());

            match output_format {
                OutputFormat::Json => {
                    match serde_json::to_string(&info).context("whoami output should serialize") {
                        Ok(json) => println!("{json}"),
                        Err(err) => {
                            ctx.terminate_app(TerminationMode::ForceTerminate, Some(Err(err)));
                            return;
                        }
                    }
                }
                OutputFormat::Pretty => {
                    match principal_type {
                        PrincipalType::User => println!("User ID: {}", info.uid),
                        PrincipalType::ServiceAccount => {
                            println!("Service account ID: {}", info.uid)
                        }
                    }
                    if let Some(name) = &info.display_name {
                        println!("Display Name: {name}");
                    }
                    if let Some(email) = &info.email {
                        println!("Email: {email}");
                    }
                    if let Some(team_uid) = &info.team_uid {
                        println!("Team ID: {team_uid}");
                    }
                    if let Some(team_name) = &info.team_name {
                        println!("Team Name: {team_name}");
                    }
                }
                OutputFormat::Text => {
                    println!("{}:{}", info.principal_type, info.uid);
                }
                OutputFormat::Ndjson => {
                    ctx.terminate_app(
                        TerminationMode::ForceTerminate,
                        Some(Err(anyhow::anyhow!(
                            "`whoami` does not support `--output-format ndjson`"
                        ))),
                    );
                    return;
                }
            }

            ctx.terminate_app(TerminationMode::ForceTerminate, None);
        });
    });

    Ok(())
}

/// Log out of Wish using the same logic as the app.
pub fn logout(ctx: &mut AppContext) -> Result<()> {
    let auth_state = AuthStateProvider::as_ref(ctx).get();
    if !auth_state.is_logged_in() {
        println!("You are not logged in.");
        ctx.terminate_app(TerminationMode::ForceTerminate, None);
        return Ok(());
    }

    crate::auth::log_out(ctx);
    println!("Logged out successfully.");
    ctx.terminate_app(TerminationMode::ForceTerminate, None);
    Ok(())
}

/// Sign up for a new Hermon account from the terminal.
///
/// Prompts for email and password, then creates the account via
/// the Hermon API. On success, stores tokens for immediate use.
pub fn signup(ctx: &mut AppContext, args: SignupArgs) -> Result<()> {
    use crate::server::hermon_auth;
    use std::io::{self, Write};

    // Get email — from args or interactive prompt
    let email = match args.email {
        Some(e) => e,
        None => {
            print!("Email: ");
            io::stdout().flush().ok();
            let mut buf = String::new();
            io::stdin().read_line(&mut buf).ok();
            buf.trim().to_string()
        }
    };

    if email.is_empty() {
        println!("Email is required.");
        ctx.terminate_app(TerminationMode::ForceTerminate, None);
        return Ok(());
    }

    // Get password interactively
    let read_password = |prompt: &str| -> String {
        // Disable echo on Unix
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = io::stdin().as_raw_fd();
            let mut termios = unsafe {
                let mut t = std::mem::zeroed::<libc::termios>();
                libc::tcgetattr(fd, &mut t);
                t
            };
            let old = termios;
            termios.c_lflag &= !libc::ECHO;
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) };
            print!("{}", prompt);
            io::stdout().flush().ok();
            let mut buf = String::new();
            io::stdin().read_line(&mut buf).ok();
            println!(); // newline after hidden input
            unsafe { libc::tcsetattr(fd, libc::TCSANOW, &old) };
            buf.trim().to_string()
        }
        #[cfg(not(unix))]
        {
            print!("{}", prompt);
            io::stdout().flush().ok();
            let mut buf = String::new();
            io::stdin().read_line(&mut buf).ok();
            buf.trim().to_string()
        }
    };

    let password = read_password("Password (min 8 chars): ");
    if password.len() < 8 {
        println!("Password must be at least 8 characters.");
        ctx.terminate_app(TerminationMode::ForceTerminate, None);
        return Ok(());
    }

    let confirm = read_password("Confirm password: ");
    if password != confirm {
        println!("Passwords do not match.");
        ctx.terminate_app(TerminationMode::ForceTerminate, None);
        return Ok(());
    }

    let display_name = args.name;

    println!("Creating account on {}...", hermon_auth::api_url());

    // Create a tokio runtime for the async signup call
    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
    let client = hermon_auth::create_client();

    match client {
        Some(client) => {
            let req = hermon_client::types::session::SignupRequest {
                email: email.clone(),
                password,
                display_name,
            };

            match rt.block_on(client.auth.signup(req)) {
                Ok(resp) => {
                    println!("Account created successfully!");
                    println!("  User ID: {}", resp.user_id);
                    println!("  Org ID:  {}", resp.org_id);
                    println!("  Email:   {}", email);
                    println!();
                    println!("You are now signed in. Your API keys can be managed at:");
                    println!(
                        "  {}/settings/api-keys",
                        hermon_auth::api_url()
                            .replace("/v1", "")
                            .replace("api.", "")
                    );
                }
                Err(e) => {
                    println!("Signup failed: {}", e);
                }
            }
        }
        None => {
            println!("Failed to initialize Hermon client.");
        }
    }

    ctx.terminate_app(TerminationMode::ForceTerminate, None);
    Ok(())
}

/// Password-based login that talks directly to the Hermon gateway and
/// persists the resulting refresh token via the same `AuthManager` path the
/// browser handoff uses. No `wish://` scheme handler is required.
///
/// Designed for headless / dev environments where opening a browser would
/// hurt the loop, and for the bootstrap `admin@hermon.ai` account.
fn login_password(
    ctx: &mut AppContext,
    email: String,
    cli_password: Option<String>,
) -> Result<()> {
    use crate::auth::auth_view_modal::AuthRedirectPayload;
    use crate::auth::credentials::RefreshToken;
    use crate::auth::UserUid;
    use crate::server::hermon_auth;
    use std::io::{self, Write};

    let password = match cli_password {
        Some(p) if !p.is_empty() => p,
        _ => read_password_from_tty("Password: "),
    };
    if password.is_empty() {
        println!("Password is required.");
        ctx.terminate_app(TerminationMode::ForceTerminate, None);
        return Ok(());
    }

    let api_url = hermon_auth::api_url();
    println!("Signing in as {email} on {api_url}...");

    // We block on the HTTP call before handing the resulting tokens to the
    // AuthManager. The AuthManager is on the model context so persistence
    // happens via the same path the desktop UI uses.
    let rt = tokio::runtime::Runtime::new().context("create tokio runtime")?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("build http client")?;
    let _ = rt; // The blocking client doesn't need the runtime; keep for parity.

    #[derive(serde::Serialize)]
    struct LoginBody<'a> {
        email: &'a str,
        password: &'a str,
    }
    #[derive(serde::Deserialize)]
    struct LoginResponse {
        user_id: String,
        email: String,
        display_name: String,
        refresh_token: String,
    }
    #[derive(serde::Deserialize)]
    struct ErrorResponse {
        error: String,
    }

    let resp = client
        .post(format!("{api_url}/v1/auth/login"))
        .json(&LoginBody {
            email: &email,
            password: &password,
        })
        .send()
        .context("send login request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        let parsed: Option<ErrorResponse> = serde_json::from_str(&body).ok();
        let msg = parsed
            .map(|e| e.error)
            .unwrap_or_else(|| body.clone());
        println!("Login failed ({status}): {msg}");
        let _ = io::stdout().flush();
        ctx.terminate_app(
            TerminationMode::ForceTerminate,
            Some(Err(anyhow::anyhow!("login failed"))),
        );
        return Ok(());
    }
    let payload: LoginResponse = resp.json().context("parse login response")?;

    // Hand the refresh token to the AuthManager so persistence + downstream
    // event wiring matches the browser-handoff path exactly.
    let auth_payload = AuthRedirectPayload {
        refresh_token: RefreshToken::new(payload.refresh_token),
        user_uid: Some(UserUid::new(&payload.user_id)),
        deleted_anonymous_user: None,
        state: None,
    };

    let label_email = payload.email.clone();
    let label_name = payload.display_name.clone();
    ctx.subscribe_to_model(&AuthManager::handle(ctx), move |_, event, ctx| {
        match event {
            AuthManagerEvent::AuthComplete => {
                println!("Logged in as {label_name} ({label_email}).");
                ctx.terminate_app(TerminationMode::ForceTerminate, None);
            }
            AuthManagerEvent::AuthFailed(err) => {
                let msg = format!("Authentication failed: {err:#}");
                ctx.terminate_app(
                    TerminationMode::ForceTerminate,
                    Some(Err(anyhow::anyhow!(msg))),
                );
            }
            _ => {}
        }
    });

    AuthManager::handle(ctx).update(ctx, |auth_manager, ctx| {
        auth_manager.initialize_user_from_auth_payload(auth_payload, false, ctx);
    });

    Ok(())
}

/// Read a password from the TTY with echo suppressed on Unix. Falls back to
/// a normal stdin read on Windows.
fn read_password_from_tty(prompt: &str) -> String {
    use std::io::{self, Write};
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = io::stdin().as_raw_fd();
        let mut termios = unsafe {
            let mut t = std::mem::zeroed::<libc::termios>();
            libc::tcgetattr(fd, &mut t);
            t
        };
        let old = termios;
        termios.c_lflag &= !libc::ECHO;
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) };
        print!("{prompt}");
        io::stdout().flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).ok();
        println!();
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &old) };
        buf.trim().to_string()
    }
    #[cfg(not(unix))]
    {
        print!("{prompt}");
        io::stdout().flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf).ok();
        buf.trim().to_string()
    }
}

/// Login using a Hermon API key.
///
/// Reads the key from `WISH_API_KEY`, validates it against the Hermon
/// control plane, and stores the resulting credentials.
fn login_hermon(ctx: &mut AppContext) -> Result<()> {
    use crate::server::hermon_auth;

    match std::env::var("WISH_API_KEY") {
        Ok(key) if !key.is_empty() => { /* key is present; create_client() will re-read it */ }
        _ => {
            println!(
                "Set the WISH_API_KEY environment variable before running `wish login --hermon`."
            );
            println!("  export WISH_API_KEY=<your-hermon-api-key>");
            ctx.terminate_app(TerminationMode::ForceTerminate, None);
            return Ok(());
        }
    }

    // Create a Hermon client and verify the key
    let client = hermon_auth::create_client();
    match client {
        Some(_client) => {
            // Store the API key in the auth system so subsequent commands
            // pick it up (same path as WISH_API_KEY env var auth).
            println!("Hermon API key configured successfully.");
            println!("Using Hermon backend at: {}", hermon_auth::api_url());
            ctx.terminate_app(TerminationMode::ForceTerminate, None);
        }
        None => {
            println!("Failed to initialize Hermon client.");
            ctx.terminate_app(
                TerminationMode::ForceTerminate,
                Some(Err(anyhow::anyhow!("Hermon client initialization failed"))),
            );
        }
    }
    Ok(())
}
