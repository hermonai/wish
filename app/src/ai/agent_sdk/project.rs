//! `wish project` CLI runner — talks to the Hermon gateway's
//! `/v1/projects` surface.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use wishui::{platform::TerminationMode, AppContext, SingletonEntity};

use wish_cli::project::{AddArgs, ProjectCommand, RunArgs, ShowArgs};

use crate::auth::AuthStateProvider;
use crate::server::hermon_auth;

/// Server wire shape — matches `routes::projects::Project` in hermon-gateway.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Project {
    id: String,
    user_id: String,
    #[serde(default)]
    workspace_id: Option<String>,
    name: String,
    root_path: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    build_cmd: Option<String>,
    #[serde(default)]
    test_cmd: Option<String>,
    #[serde(default)]
    run_cmd: Option<String>,
    #[serde(default)]
    lint_cmd: Option<String>,
    #[serde(default)]
    format_cmd: Option<String>,
    #[serde(default)]
    extras: serde_json::Value,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Deserialize)]
struct ProjectList {
    items: Vec<Project>,
}

pub fn run(ctx: &mut AppContext, command: ProjectCommand) -> Result<()> {
    let bearer = bearer_from_auth_state(ctx)?;
    let api_url = hermon_auth::api_url();
    let result = match command {
        ProjectCommand::List => list_projects(&api_url, &bearer),
        ProjectCommand::Add(args) => add_project(&api_url, &bearer, args),
        ProjectCommand::Show(args) => show_project(&api_url, &bearer, args),
        ProjectCommand::Rm(args) => rm_project(&api_url, &bearer, args),
        ProjectCommand::Run(args) => run_command(&api_url, &bearer, args),
    };
    let term_result = result.map_err(|e| anyhow!(format!("{e:#}")));
    ctx.terminate_app(
        TerminationMode::ForceTerminate,
        if term_result.is_err() {
            Some(term_result.map(|_| ()))
        } else {
            None
        },
    );
    Ok(())
}

/// Recover the user's Hermon credentials from the in-process auth state.
/// `wish login --email` stores a Bearer credential keyed off the Hermon
/// refresh token; that's exactly what the gateway accepts for
/// `/v1/auth/me` and `/v1/projects`.
fn bearer_from_auth_state(ctx: &AppContext) -> Result<String> {
    let auth_state = AuthStateProvider::as_ref(ctx).get();
    let creds = auth_state
        .credentials()
        .context("not signed in — run `wish login --email <you@example.com>` first")?;
    // The Hermon-native sign-in path stashes the refresh token as a
    // Bearer credential (see auth_manager::sign_in_with_hermon_account).
    match creds {
        crate::auth::credentials::Credentials::Bearer(token) => Ok(token),
        crate::auth::credentials::Credentials::ApiKey { key, .. } => Ok(key),
        _ => Err(anyhow!(
            "current credentials aren't Hermon-native; run `wish login --email <you@example.com>` to switch"
        )),
    }
}

fn http() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("build http client")
}

fn list_projects(api_url: &str, bearer: &str) -> Result<()> {
    let resp = http()?
        .get(format!("{api_url}/v1/projects"))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/projects")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    let list: ProjectList = resp.json().context("parse /v1/projects")?;
    if list.items.is_empty() {
        println!("No projects yet. Add one with `wish project add <root_path>`.");
        return Ok(());
    }
    for p in &list.items {
        println!("{}  {}", p.name, p.root_path);
        if let Some(lang) = &p.language {
            println!("    language: {lang}");
        }
        for (label, cmd) in [
            ("build ", &p.build_cmd),
            ("test  ", &p.test_cmd),
            ("run   ", &p.run_cmd),
            ("lint  ", &p.lint_cmd),
            ("format", &p.format_cmd),
        ] {
            if let Some(c) = cmd {
                println!("    {label}: {c}");
            }
        }
        println!("    id: {}", p.id);
    }
    Ok(())
}

#[derive(Serialize)]
struct CreateBody<'a> {
    root_path: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_cmd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_cmd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_cmd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lint_cmd: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format_cmd: Option<&'a str>,
}

fn add_project(api_url: &str, bearer: &str, args: AddArgs) -> Result<()> {
    let body = CreateBody {
        root_path: &args.root,
        name: args.name.as_deref(),
        language: args.language.as_deref(),
        build_cmd: args.build.as_deref(),
        test_cmd: args.test.as_deref(),
        run_cmd: args.run.as_deref(),
        lint_cmd: args.lint.as_deref(),
        format_cmd: args.format.as_deref(),
    };
    let resp = http()?
        .post(format!("{api_url}/v1/projects"))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .context("POST /v1/projects")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    let p: Project = resp.json().context("parse /v1/projects")?;
    println!("Saved project {} at {}", p.name, p.root_path);
    println!("  id: {}", p.id);
    Ok(())
}

/// Resolve `name|id` to a single Project, fetching the user's list.
fn resolve(api_url: &str, bearer: &str, identifier: &str) -> Result<Project> {
    let resp = http()?
        .get(format!("{api_url}/v1/projects"))
        .bearer_auth(bearer)
        .send()
        .context("GET /v1/projects")?;
    let list: ProjectList = resp.json().context("parse /v1/projects")?;
    let needle = identifier.trim();
    list.items
        .into_iter()
        .find(|p| p.id == needle || p.name.eq_ignore_ascii_case(needle))
        .ok_or_else(|| anyhow!("no project named or id'd '{needle}'"))
}

fn show_project(api_url: &str, bearer: &str, args: ShowArgs) -> Result<()> {
    let p = resolve(api_url, bearer, &args.project)?;
    println!("{}", serde_json::to_string_pretty(&p)?);
    Ok(())
}

fn rm_project(api_url: &str, bearer: &str, args: ShowArgs) -> Result<()> {
    let p = resolve(api_url, bearer, &args.project)?;
    let resp = http()?
        .delete(format!("{api_url}/v1/projects/{}", p.id))
        .bearer_auth(bearer)
        .send()
        .context("DELETE /v1/projects/:id")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        return Err(anyhow!("HTTP {status}: {body}"));
    }
    println!("Removed project {} ({})", p.name, p.id);
    Ok(())
}

fn run_command(api_url: &str, bearer: &str, args: RunArgs) -> Result<()> {
    let p = match args.project {
        Some(s) => resolve(api_url, bearer, &s)?,
        None => {
            let resp = http()?
                .get(format!("{api_url}/v1/projects"))
                .bearer_auth(bearer)
                .send()?;
            let list: ProjectList = resp.json()?;
            list.items
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("no projects — add one with `wish project add`"))?
        }
    };
    let cmd_opt = match args.command.as_str() {
        "build" => &p.build_cmd,
        "test" => &p.test_cmd,
        "run" => &p.run_cmd,
        "lint" => &p.lint_cmd,
        "format" => &p.format_cmd,
        other => {
            return Err(anyhow!(
                "unknown SDLC command '{other}'; expected build|test|run|lint|format"
            ));
        }
    };
    let cmd_str = cmd_opt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            anyhow!(
                "project {} has no {} command; set it with `wish project add --root {} --{} \"...\"`",
                p.name,
                args.command,
                p.root_path,
                args.command
            )
        })?;
    println!("→ {} in {}", cmd_str, p.root_path);
    // Run synchronously in the user's shell so the command's own
    // formatting (cargo's coloured output, etc.) reaches the terminal.
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd_str)
        .current_dir(&p.root_path)
        .status()
        .with_context(|| format!("spawn {cmd_str}"))?;
    if !status.success() {
        return Err(anyhow!("`{cmd_str}` exited with status {status}"));
    }
    Ok(())
}
