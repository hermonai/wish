#![cfg_attr(target_family = "wasm", allow(dead_code))]

use std::{
    env, fmt,
    path::{Path, PathBuf},
};

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use url::Url;

use wish_core::channel::ChannelState;
use wish_core::features::FeatureFlag;

use crate::agent::OutputFormat;

#[cfg(windows)]
mod process_handle;

pub mod artifact;
pub mod scope;
pub mod skill;

pub mod agent;
pub mod completions;
pub mod config_file;
pub mod environment;
pub mod federate;
pub mod harness_support;
pub mod integration;
pub mod json_filter;
pub mod mcp;
pub mod i18n;
pub mod model;
pub mod project;
pub mod tensor;
pub mod provider;
pub mod schedule;
pub mod secret;
pub mod share;
pub mod task;
pub const WISH_RUN_ID_ENV: &str = "WISH_RUN_ID";
pub const WISH_PARENT_RUN_ID_ENV: &str = "WISH_PARENT_RUN_ID";
pub const WISH_CLI_ENV: &str = "WISH_CLI";
pub const WISH_HARNESS_ENV: &str = "WISH_HARNESS";
pub const SERVER_ROOT_URL_OVERRIDE_ENV: &str = "WISH_SERVER_ROOT_URL";
pub const WS_SERVER_URL_OVERRIDE_ENV: &str = "WISH_WS_SERVER_URL";
pub const SESSION_SHARING_SERVER_URL_OVERRIDE_ENV: &str = "WISH_SESSION_SHARING_SERVER_URL";

/// Options related to the parent process that spawned this Wish instance.
#[derive(Debug, Default, Clone, clap::Args)]
pub struct ParentOpts {
    /// The ID of the Wish process that spawned this one.
    ///
    /// Used by codepaths that attempt to detect when the parent Wish process
    /// has terminated. Guaranteed to be [`None`] when this is the initial
    /// Wish process, but may also be [`None`] for Wish child processes if the
    /// child process doesn't need to keep track of its parent.
    #[arg(long = "parent-pid", hide = true)]
    pub pid: Option<u32>,

    /// A handle to our parent process.
    ///
    /// Used on Windows for crash recovery instead of parent_pid, as process
    /// IDs can be reused, so a process handle is more robust.
    #[cfg(windows)]
    #[arg(long = "parent-handle", hide = true)]
    pub handle: Option<process_handle::ProcessHandle>,
}

/// Hidden worker args used to scope remote-server proxy/daemon sockets by
/// Wish identity without exposing credentials.
#[derive(Debug, Clone, Default, clap::Args)]
pub struct RemoteServerIdentityArgs {
    /// Non-secret identity partition key for the remote-server daemon.
    #[arg(long = "identity-key", hide = true)]
    pub identity_key: String,
}

/// Global options that apply to all CLI commands.
#[derive(Debug, Default, Clone, clap::Args)]
pub struct GlobalOptions {
    /// API key for server authentication.
    #[arg(long = "api-key", global = true, env = "WISH_API_KEY")]
    pub api_key: Option<String>,

    /// Set the output format.
    #[arg(
        long = "output-format",
        global = true,
        value_enum,
        default_value_t = OutputFormat::Pretty,
        env = "WISH_OUTPUT_FORMAT"
    )]
    pub output_format: OutputFormat,
}

/// Command-line argument parser for the main Wish binary. This is used across all channels.
#[derive(Debug, Default, Parser, Clone)]
#[command(
    name = "wish",
    display_name = "Wish",
    about = r#"The agentic development environment

The Wish CLI is a tool for running, managing, and orchestrating coding agents at scale.
Use the CLI to:
* Launch and inspect cloud agents
* Schedule cloud agents to run in the future
* Manage the environments that cloud agents run in
* Upload secrets to secure storage"#
)]
#[clap(args_conflicts_with_subcommands = true)]
pub struct Args {
    #[clap(flatten)]
    global_options: GlobalOptions,

    /// Enable debug mode.
    #[arg(long = "debug", global = true, help = "Enable debug logging")]
    debug: bool,

    /// Override the server root URL.
    #[arg(
        long = "server-root-url",
        global = true,
        hide = true,
        env = "WISH_SERVER_ROOT_URL"
    )]
    server_root_url: Option<String>,

    /// Override the websocket server URL.
    #[arg(
        long = "ws-server-url",
        global = true,
        hide = true,
        env = "WISH_WS_SERVER_URL"
    )]
    ws_server_url: Option<String>,

    /// Override the session sharing server URL.
    #[arg(
        long = "session-sharing-server-url",
        global = true,
        hide = true,
        env = "WISH_SESSION_SHARING_SERVER_URL"
    )]
    session_sharing_server_url: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,

    #[clap(flatten)]
    args: AppArgs,
}

/// Flags for the Wish application. Additional binaries, like test runners, may use this type
/// along with their own flags, or convert their flags into an `AppArgs` value.
#[derive(Debug, Default, clap::Args, Clone)]
pub struct AppArgs {
    /// True if this instance of Wish was launched at the end of the auto-update process.
    #[arg(long = "finish-update", hide = true)]
    pub finish_update: bool,

    /// Crash recovery mechanism to use if we detect the parent process terminated.
    #[cfg(enable_crash_recovery)]
    #[arg(long = "crash-recovery-mechanism", value_enum, requires = "ParentOpts")]
    pub crash_recovery_mechanism: Option<RecoveryMechanism>,

    /// Options related to the parent process that spawned this Wish instance.
    #[clap(flatten)]
    pub parent: ParentOpts,

    /// URLs to open in Wish.
    #[arg(hide = true)]
    pub urls: Vec<Url>,

    /// Open the given folder as the active workspace project on launch.
    /// Equivalent to running "Open Folder…" from the command palette after startup.
    #[arg(
        long = "folder",
        short = 'd',
        value_name = "PATH",
        value_hint = clap::ValueHint::DirPath,
    )]
    pub folder: Option<PathBuf>,

    /// Open the given file in a code pane on launch. Repeatable.
    /// Accepts an optional line/column suffix: `path/to/file.rs:42:5`.
    /// When no `--folder` is given, the file's canonical parent directory
    /// is used as the workspace project root.
    #[arg(
        long = "file",
        short = 'F',
        value_name = "PATH",
        value_hint = clap::ValueHint::FilePath,
    )]
    pub files: Vec<String>,
}

/// Rewrite `wish PATH …` into `wish --folder PATH …` or `wish --file PATH …`
/// so users can launch a workspace with `wish .` (folder) or open a file with
/// `wish src/main.rs:42:5` the way they do with editors.
///
/// The rewrite is deliberately conservative: each candidate positional is only
/// rewritten when it is unambiguously a filesystem path — `.`, `..`, anything
/// containing `/` (or `\` on Windows), anything starting with `~`, or an
/// existing on-disk file/directory (possibly with a `:LINE[:COL]` suffix).
/// Anything else is left alone so subcommands (`wish agent …`) and URL
/// positionals (`wish wish://…`) keep working untouched.
///
/// Multiple file positionals are accepted: `wish src/a.rs src/b.rs` opens both.
/// A directory positional must come first if mixed with files.
fn rewrite_path_positional(argv: Vec<String>) -> Vec<String> {
    // Find the index of the first non-flag arg after the binary name. Anything
    // before that (flags) is left untouched; anything from that point on is
    // considered for rewriting.
    let Some(first_idx) = argv
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(i, a)| if a.starts_with('-') { None } else { Some(i) })
    else {
        return argv;
    };

    let first = &argv[first_idx];

    // If --folder is already explicit, the user is driving clap directly. Don't
    // reclassify subsequent tokens (the next one may be the *value* of --folder).
    let folder_already_explicit = argv.iter().any(|a| {
        a == "--folder" || a == "-d" || a.starts_with("--folder=") || a.starts_with("-d=")
    });
    if folder_already_explicit {
        return argv;
    }

    // If the first non-flag arg is clearly a URL (contains `://`), leave the
    // whole tail alone so URL positionals like `wish://launch` still flow
    // through the urls vector. We deliberately use a strict `://` substring
    // check instead of `Url::parse`, because Url's grammar accepts things like
    // `Cargo.toml:10` as scheme=`Cargo.toml`, which would mis-classify files
    // with `:LINE[:COL]` suffixes.
    if first.contains("://") {
        return argv;
    }

    // Likewise leave subcommands alone: if `first` doesn't look path-shaped and
    // isn't an existing path, we assume it's a subcommand and bail.
    if !looks_like_path(first) {
        return argv;
    }

    // We're going to rewrite at least the first positional. Preserve everything
    // before it (binary name + any pre-positional flags) verbatim, then walk the
    // remaining args splitting flags-passthrough from positionals-to-classify.
    let mut rewritten: Vec<String> = argv[..first_idx].to_vec();
    let mut folder_seen = false;

    let mut i = first_idx;
    while i < argv.len() {
        let arg = &argv[i];

        // Once we hit a flag, stop reclassifying positionals — everything after
        // belongs to whatever flag context follows. Forward the rest verbatim.
        if arg.starts_with('-') {
            rewritten.extend(argv[i..].iter().cloned());
            break;
        }

        let classification = classify_path_positional(arg);
        match classification {
            PathPositional::Directory => {
                if folder_seen {
                    // Already have a folder — treat extras as files (so
                    // `wish proj/ extra/` doesn't silently drop the second).
                    rewritten.push("--file".to_string());
                    rewritten.push(arg.clone());
                } else {
                    rewritten.push("--folder".to_string());
                    rewritten.push(arg.clone());
                    folder_seen = true;
                }
            }
            PathPositional::File => {
                rewritten.push("--file".to_string());
                rewritten.push(arg.clone());
            }
            PathPositional::Unknown => {
                // Looked path-shaped but doesn't exist on disk. Prefer treating
                // as a folder for the first such arg (matches `wish ./new-proj`
                // for a not-yet-created dir); subsequent unknowns are files.
                if folder_seen {
                    rewritten.push("--file".to_string());
                    rewritten.push(arg.clone());
                } else {
                    rewritten.push("--folder".to_string());
                    rewritten.push(arg.clone());
                    folder_seen = true;
                }
            }
        }

        i += 1;
    }

    rewritten
}

enum PathPositional {
    Directory,
    File,
    Unknown,
}

/// Quick path-shape test. Returns true for anything that clearly names a
/// filesystem location (existing or not). Subcommands like `agent` or `mcp`
/// fall through to false because they don't carry path metacharacters.
fn looks_like_path(s: &str) -> bool {
    if s == "." || s == ".." {
        return true;
    }
    if s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with('~')
        || s.starts_with('/')
        || s.contains('/')
    {
        return true;
    }
    if cfg!(windows) && s.contains('\\') {
        return true;
    }
    let raw = std::path::Path::new(s);
    if raw.is_dir() || raw.is_file() {
        return true;
    }
    // Allow `wish foo.rs:42:5` for files in cwd: strip a trailing `:N` or `:N:N`
    // suffix and re-check existence.
    if let Some((base, _)) = split_line_column_suffix(s) {
        let p = std::path::Path::new(base);
        if p.is_file() || p.is_dir() {
            return true;
        }
    }
    false
}

fn classify_path_positional(s: &str) -> PathPositional {
    let raw = std::path::Path::new(s);
    if raw.is_dir() {
        return PathPositional::Directory;
    }
    if raw.is_file() {
        return PathPositional::File;
    }
    // Try stripping `:LINE[:COL]` for files like `src/main.rs:42:5`.
    if let Some((base, _)) = split_line_column_suffix(s) {
        let p = std::path::Path::new(base);
        if p.is_file() {
            return PathPositional::File;
        }
        if p.is_dir() {
            return PathPositional::Directory;
        }
    }
    PathPositional::Unknown
}

/// Split a `path:line` or `path:line:column` suffix off the end of `s`.
/// Returns `Some((base, suffix))` if a numeric `:line[:col]` is detected.
/// Conservative — only accepts pure digits to avoid mangling Windows drive
/// letters like `C:\Users\…` (the colon there is followed by `\`, not digits).
fn split_line_column_suffix(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();

    let mut saw_digit = false;
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
        saw_digit = true;
    }
    if !saw_digit || i == 0 || bytes[i - 1] != b':' {
        return None;
    }
    // First (rightmost) digit run found. Now see if there's another `:NUM`
    // group before it (the line in `file:line:col`).
    let mut j = i - 1;
    let mut saw_inner_digit = false;
    while j > 0 && bytes[j - 1].is_ascii_digit() {
        j -= 1;
        saw_inner_digit = true;
    }
    if saw_inner_digit && j > 0 && bytes[j - 1] == b':' {
        // Form: PATH:LINE:COL
        Some((&s[..j - 1], &s[j - 1..]))
    } else {
        // Form: PATH:LINE
        Some((&s[..i - 1], &s[i - 1..]))
    }
}

impl Args {
    /// Parses command-line arguments from the operating environment. May exit early if arguments
    /// are incorrectly specified.
    pub fn from_env() -> Self {
        cfg_if::cfg_if! {
            // wasm doesn't have any concept of an environment, so skip parsing and return defaults
            if #[cfg(target_family = "wasm")] {
                Args::default()
            } else {
                use clap::FromArgMatches as _;

                // Check for disabled commands before parsing to prevent help from showing (e.g.
                // `warp environment` should not return help text)
                if !FeatureFlag::CloudEnvironments.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "environment" {
                        eprintln!("error: unrecognized subcommand 'environment'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                if !FeatureFlag::ProviderCommand.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "provider" {
                        eprintln!("error: unrecognized subcommand 'provider'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                if !FeatureFlag::IntegrationCommand.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "integration" {
                        eprintln!("error: unrecognized subcommand 'integration'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                if !FeatureFlag::ScheduledAmbientAgents.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "schedule" {
                        eprintln!("error: unrecognized subcommand 'schedule'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                if !FeatureFlag::WarpManagedSecrets.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "secret" {
                        eprintln!("error: unrecognized subcommand 'secret'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                if !FeatureFlag::HermonIdentityFederation.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "federate" {
                        eprintln!("error: unrecognized subcommand 'federate'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                if !FeatureFlag::ArtifactCommand.is_enabled() {
                    let args: Vec<String> = env::args().collect();
                    if args.len() > 1 && args[1] == "artifact" {
                        eprintln!("error: unrecognized subcommand 'artifact'\n");
                        eprintln!("For more information, try '--help'");
                        std::process::exit(2);
                    }
                }

                let command = Self::clap_command();

                // Allow `wish [PATH]` (e.g. `wish .`, `wish ./project`, `wish /abs/path`)
                // by rewriting a bare path positional to `--folder PATH` before clap parses.
                // We only rewrite when the arg is unambiguously a filesystem path so this
                // doesn't shadow subcommands like `wish agent` or URL positionals.
                let argv = rewrite_path_positional(env::args().collect());

                command.try_get_matches_from(argv)
                    .and_then(|matches| Self::from_arg_matches(&matches))
                    .unwrap_or_else(|err| {
                        // We attach a console to ensure help and error messages are printed
                        // when using the CLI.
                        #[cfg(windows)]
                        wish_util::windows::attach_to_parent_console();
                        err.exit()
                    })
            }
        }
    }

    /// Construct the [`clap::Command`] that backs `Args`.
    ///
    /// IMPORTANT: use this instead of [`CommandFactory::command`], since we customize the command at runtime.
    pub fn clap_command() -> clap::Command {
        let mut command = <Args as CommandFactory>::command();

        // Hide the environment subcommands and --environment flags from help text
        if !FeatureFlag::CloudEnvironments.is_enabled() {
            command = command.mut_subcommand("environment", |c| c.hide(true));
            command = command.mut_subcommand("agent", |agent_cmd| {
                agent_cmd
                    .mut_subcommand("run", |run_cmd| {
                        run_cmd.mut_arg("environment", |arg| arg.hide(true))
                    })
                    .mut_subcommand("run-cloud", |cloud_cmd| {
                        cloud_cmd.mut_arg("environment", |arg| arg.hide(true))
                    })
            });
        }

        // Hide the --conversation flag from help text
        if !FeatureFlag::CloudConversations.is_enabled() {
            command = command.mut_subcommand("agent", |agent_cmd| {
                agent_cmd
                    .mut_subcommand("run", |run_cmd| {
                        run_cmd.mut_arg("conversation", |arg| arg.hide(true))
                    })
                    .mut_subcommand("run-cloud", |cloud_cmd| {
                        cloud_cmd.mut_arg("conversation", |arg| arg.hide(true))
                    })
            });
        }

        if !FeatureFlag::AmbientAgentsCommandLine.is_enabled() {
            command = command.mut_subcommand("agent", |agent_cmd| {
                agent_cmd.mut_subcommand("run-cloud", |c| c.hide(true))
            });
        }

        // Hide the provider subcommand from help text
        if !FeatureFlag::ProviderCommand.is_enabled() {
            command = command.mut_subcommand("provider", |c| c.hide(true));
        }

        // Hide the integration subcommand from help text
        if !FeatureFlag::IntegrationCommand.is_enabled() {
            command = command.mut_subcommand("integration", |c| c.hide(true));
        }

        // Hide the schedule subcommand from help text.
        if !FeatureFlag::ScheduledAmbientAgents.is_enabled() {
            command = command.mut_subcommand("schedule", |c| c.hide(true));
        }

        // Hide the secret subcommand from help text.
        if !FeatureFlag::WarpManagedSecrets.is_enabled() {
            command = command.mut_subcommand("secret", |c| c.hide(true));
        }

        // Hide the federate subcommand from help text.
        if !FeatureFlag::HermonIdentityFederation.is_enabled() {
            command = command.mut_subcommand("federate", |c| c.hide(true));
        }

        // Hide the harness-support subcommand from help text.
        if !FeatureFlag::AgentHarness.is_enabled() {
            command = command.mut_subcommand("harness-support", |c| c.hide(true));
        }

        // Hide the conversation subcommand and --conversation flag from help text.
        if !FeatureFlag::ConversationApi.is_enabled() {
            command = command.mut_subcommand("run", |run_cmd| {
                run_cmd
                    .mut_subcommand("conversation", |c| c.hide(true))
                    .mut_subcommand("get", |get_cmd| {
                        get_cmd.mut_arg("conversation", |arg| arg.hide(true))
                    })
            });
        }
        // Hide the message subcommand from help text.
        if !FeatureFlag::OrchestrationV2.is_enabled() {
            command = command.mut_subcommand("run", |run_cmd| {
                run_cmd.mut_subcommand("message", |c| c.hide(true))
            });
        }

        // Hide the artifact subcommand from help text.
        if !FeatureFlag::ArtifactCommand.is_enabled() {
            command = command.mut_subcommand("artifact", |c| c.hide(true));
        }

        // Wire up `--version` / `-V` using the same version metadata used elsewhere in the
        // app, so the CLI reports the build's release tag.
        command = command.version(version_string());

        // Substitute the actual binary name into help output. Ideally clap would do this for us.
        let bin_name =
            binary_name().unwrap_or_else(|| ChannelState::channel().cli_command_name().to_string());
        command = command.after_help(color_print::cformat!(
            r#"<bold><underline>Examples:</underline></bold>

  <dim>$</dim> <bold>{bin_name} agent run --prompt "Build anything"</bold>

  <dim>$</dim> <bold>{bin_name} mcp list</bold>

<bold><underline>Learn more:</underline></bold>
* Use <bold>{bin_name} help</bold> to learn more about each command
* Read the documentation at https://wish.hermon.ai/docs/reference/cli
"#
        ));

        command
    }

    /// The requested subcommand, if any.
    pub fn command(&self) -> Option<&Command> {
        self.command.as_ref()
    }

    /// Args for the main Wish application, if not running a subcommand.
    pub fn app_args(&self) -> &AppArgs {
        &self.args
    }

    /// Extract the main Wish application args.
    pub fn into_app_args(self) -> AppArgs {
        self.args
    }

    /// Returns the global options.
    pub fn global_options(&self) -> &GlobalOptions {
        &self.global_options
    }

    /// Returns the API key if provided.
    pub fn api_key(&self) -> Option<&String> {
        self.global_options.api_key.as_ref()
    }

    /// Returns the output format.
    pub fn output_format(&self) -> OutputFormat {
        self.global_options.output_format
    }

    /// Returns true if debug logging is enabled.
    pub fn debug(&self) -> bool {
        self.debug
    }

    pub fn server_root_url(&self) -> Option<&str> {
        self.server_root_url.as_deref()
    }

    pub fn ws_server_url(&self) -> Option<&str> {
        self.ws_server_url.as_deref()
    }

    pub fn session_sharing_server_url(&self) -> Option<&str> {
        self.session_sharing_server_url.as_deref()
    }
}

/// Warp may spawn several worker processes - mostly servers that support the main application.
///
/// These subcommands run those worker processes, which are bundled into the Warp binary.
#[derive(Debug, Clone, Subcommand)]
pub enum WorkerCommand {
    /// Run the terminal server.
    #[clap(hide = true)]
    #[cfg(unix)]
    TerminalServer(TerminalServerArgs),

    /// Run this process as the plugin host rather than the main app.
    #[cfg(feature = "plugin_host")]
    #[clap(long_flag = "plugin-host")]
    PluginHost {
        #[clap(flatten)]
        parent: ParentOpts,
    },

    /// Run the minidump server.
    #[clap(hide = true)]
    MinidumpServer {
        /// Socket name for the minidump server.
        socket_name: std::path::PathBuf,
    },

    /// Run the remote development server proxy over SSH stdio.
    /// Ensures the daemon is running, then bridges its stdin/stdout
    /// to the daemon via a Unix domain socket.
    #[cfg(not(target_family = "wasm"))]
    #[clap(hide = true)]
    RemoteServerProxy(RemoteServerIdentityArgs),

    /// Run the long-lived remote development server daemon.
    /// Listens on a Unix domain socket and accepts multiple concurrent
    /// connections from proxy processes.
    #[cfg(not(target_family = "wasm"))]
    #[clap(hide = true)]
    RemoteServerDaemon(RemoteServerIdentityArgs),

    /// Run a headless ripgrep search worker.
    #[cfg(not(target_family = "wasm"))]
    #[clap(hide = true)]
    RipgrepSearch {
        #[clap(flatten)]
        parent: ParentOpts,
        #[clap(long = "ignore-case")]
        ignore_case: bool,
        #[clap(long = "multiline")]
        multiline: bool,
        /// Search pattern.
        pattern: String,
        /// Paths to search.
        paths: Vec<std::path::PathBuf>,
    },
}

/// Arguments for the `wish login` command.
#[derive(Debug, Clone, Default, Parser)]
pub struct LoginArgs {
    /// Authenticate using a Hermon API key instead of the default device flow.
    /// The key can be provided via WISH_API_KEY or entered interactively.
    #[clap(long)]
    pub hermon: bool,

    /// Email of an existing Hermon account. When provided, password-based
    /// auth is used (no browser handoff). Prompts for the password unless
    /// `--password` is also given.
    #[clap(long)]
    pub email: Option<String>,

    /// Password for password-based login. Avoid passing on the command line
    /// in shared environments — prefer the interactive prompt or set the
    /// `WISH_PASSWORD` env var.
    #[clap(long, env = "WISH_PASSWORD")]
    pub password: Option<String>,
}

/// Arguments for the `wish signup` command.
#[derive(Debug, Clone, Default, Parser)]
pub struct SignupArgs {
    /// Email address for the new account.
    #[clap(long)]
    pub email: Option<String>,
    /// Display name (optional).
    #[clap(long)]
    pub name: Option<String>,
}

/// CLI-related subcommands. The command-line interface to Wish isn't a full SDK (e.g. with language bindings),
/// but it allows scripting some Wish functionality.
#[derive(Debug, Clone, Subcommand)]
pub enum CliCommand {
    /// Interact with Wish agents.
    #[command(subcommand)]
    Agent(crate::agent::AgentCommand),

    /// Manage cloud environments.
    #[command(subcommand)]
    Environment(crate::environment::EnvironmentCommand),

    /// Manage MCP servers.
    #[command(subcommand)]
    MCP(crate::mcp::MCPCommand),

    /// Manage runs.
    #[command(subcommand, alias = "task")]
    Run(crate::task::TaskCommand),

    /// Manage available models.
    #[command(subcommand)]
    Model(crate::model::ModelCommand),

    /// Manage project bookmarks (IDE entries with build / test / run / lint
    /// commands). Backed by the Hermon gateway's `/v1/projects` surface.
    #[command(subcommand)]
    Project(crate::project::ProjectCommand),

    /// Manage tensors stored on the Hermon gateway — the URE × wishUI
    /// substrate's data plane. Push raw bytes from a file, list /
    /// show / pull / delete, or open a tensor in the native viewer
    /// as a heatmap. Backed by `/v1/tensors`.
    #[command(subcommand)]
    Tensor(crate::tensor::TensorCommand),

    /// Language-pack tooling: list / get / install / uninstall locales,
    /// plus `translate` which asks the gateway's LLM router to generate
    /// a pack for a target locale on the fly. Backed by the Hermon
    /// gateway's `/v1/i18n*` surface.
    #[command(subcommand)]
    I18n(crate::i18n::I18nCommand),

    /// Log in to Wish.
    Login(LoginArgs),
    /// Create a new Hermon account from the terminal.
    Signup(SignupArgs),
    /// Log out of Wish.
    Logout,
    /// Print information about the logged-in user.
    Whoami,

    /// Manage providers.
    #[command(subcommand)]
    Provider(crate::provider::ProviderCommand),

    /// Manage integrations.
    #[command(subcommand)]
    Integration(crate::integration::IntegrationCommand),

    /// Create and manage scheduled Wish agents. Scheduled agents run a user-defined task periodically, according to a cron schedule.
    ///
    /// As a shorthand, the `schedule` command behaves identically to `schedule create`.
    Schedule(crate::schedule::ScheduleCommand),

    /// Manage secrets.
    #[command(subcommand)]
    Secret(crate::secret::SecretCommand),

    /// Issue and manage federated identity tokens.
    #[command(subcommand)]
    Federate(crate::federate::FederateCommand),

    /// Support commands for agent harnesses to integrate with Hermon.
    #[command(hide = true)]
    HarnessSupport(crate::harness_support::HarnessSupportArgs),

    /// Manage artifacts.
    #[command(subcommand)]
    Artifact(crate::artifact::ArtifactCommand),
}

/// A subcommand of the main Wish application. This includes all [`WorkerCommand`]s as well as app-specific debugging tools.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    #[clap(flatten)]
    Worker(WorkerCommand),

    /// Commands that make up the Wish CLI.
    #[clap(flatten)]
    CommandLine(Box<CliCommand>),

    /// Generate shell completions for your shell to stdout.
    ///
    ///
    /// For bash, add the following to ~/.bashrc:
    ///     source <(path/to/wish completions bash)
    ///
    /// For zsh, add the following to ~/.zshrc:
    ///     source <(path/to/wish completions zsh)
    ///
    /// For fish, add the following to ~/.config/fish/config.fish:
    ///     path/to/wish completions fish | source
    ///
    /// For Powershell, add the following to $PROFILE:
    ///     path\to\wish | Out-String | Invoke-Expression
    ///
    /// If no shell is provided, this defaults to the shell that Wish was run from.
    #[command(verbatim_doc_comment)]
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Option<clap_complete::aot::Shell>,
    },

    /// Print debugging information and exit.
    #[clap(long_flag = "dump-debug-info")]
    DumpDebugInfo,

    /// Print telemetry events in production and exit.
    #[clap(long_flag = "print-telemetry-events", hide = true)]
    #[cfg(not(target_family = "wasm"))]
    PrintTelemetryEvents,
}

impl Command {
    /// Whether or not the Command should print to stdout.
    pub fn prints_to_stdout(&self) -> bool {
        match self {
            Command::Worker(_) => false,
            Command::CommandLine(_) | Command::DumpDebugInfo => true,
            Command::Completions { .. } => true,
            #[cfg(not(target_family = "wasm"))]
            Command::PrintTelemetryEvents => true,
        }
    }
}

/// Arguments for the terminal server.
#[cfg(not(windows))]
#[derive(Debug, Clone, Default, clap::Args)]
pub struct TerminalServerArgs {
    #[clap(flatten)]
    pub parent: ParentOpts,
}

#[derive(Debug, Copy, Clone, clap::ValueEnum)]
pub enum RecoveryMechanism {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[value(name = "force-x11")]
    X11,
    #[value(name = "force-dedicated-gpu")]
    DedicatedGpu,
    #[value(name = "disable-opengl")]
    DisableOpenGL,
    #[value(name = "force-vulkan")]
    ForceVulkan,
}

impl fmt::Display for RecoveryMechanism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.to_possible_value().expect("no values are skipped");
        f.write_str(value.get_name())
    }
}

/// Returns the subcommand name to use for starting the terminal server.
pub fn terminal_server_subcommand() -> String {
    <Args as CommandFactory>::command()
        .find_subcommand("terminal-server")
        .expect("terminal-server subcommand not found")
        .get_name()
        .to_string()
}

/// Returns the subcommand name to use for starting the installation detection server.
pub fn installation_detection_server_subcommand() -> String {
    <Args as CommandFactory>::command()
        .find_subcommand("installation-detection-server")
        .expect("installation-detection-server subcommand not found")
        .get_name()
        .to_string()
}

/// Returns the subcommand name to use for starting the ripgrep search worker.
#[cfg(not(target_family = "wasm"))]
pub fn ripgrep_search_subcommand() -> String {
    <Args as CommandFactory>::command()
        .find_subcommand("ripgrep-search")
        .expect("ripgrep-search subcommand not found")
        .get_name()
        .to_string()
}

/// Returns the flag to use when finishing the auto-update process.
pub fn finish_update_flag() -> String {
    let command = <Args as CommandFactory>::command();
    let flag = command
        .get_arguments()
        .find(|arg| arg.get_long() == Some("finish-update"))
        .expect("finish-update flag not found")
        .get_long()
        .unwrap();
    format!("--{flag}")
}

/// Returns the flag to use for the dump-debug-info subcommand.
pub fn dump_debug_info_flag() -> String {
    let command = <Args as CommandFactory>::command();
    let flag = command
        .find_subcommand("dump-debug-info")
        .expect("dump-debug-info subcommand not found")
        .get_long_flag()
        .expect("dump-debug-info flag not found");
    format!("--{flag}")
}

/// Returns a flag that sets the current process as the parent of a Warp subcommand to spawn.
pub fn parent_flag() -> String {
    let command = <Args as CommandFactory>::command();
    let flag = command
        .get_arguments()
        .find(|arg| arg.get_long() == Some("parent-pid"))
        .expect("parent-pid flag not found")
        .get_long()
        .unwrap();
    format!("--{flag}={}", std::process::id())
}

/// The name that this binary was invoked as.
pub fn binary_name() -> Option<String> {
    // Adapted from https://github.com/clap-rs/clap/blob/2c04acd3607e5c4676477ca14948419bb31c73a1/clap_builder/src/builder/command.rs#L888-L902
    // Unfortunately, we can't use Command::get_bin_name because it's not populated until args are parsed.
    let arg0 = env::args().next()?;
    Path::new(&arg0).file_name()?.to_str().map(|s| s.to_owned())
}

/// The version string shown for `--version` / `-V`.
///
/// Sourced from [`ChannelState::app_version`], which is populated from the
/// `GIT_RELEASE_TAG` env var at compile time. Falls back to a placeholder for
/// untagged builds (e.g. local `cargo run`).
pub fn version_string() -> &'static str {
    ChannelState::app_version().unwrap_or("<unknown>")
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
