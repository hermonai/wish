//! Live workspace context injection for Wish-chat user messages.
//!
//! The premise: when the user invokes the agent, the agent should already
//! know the live state of the workspace — current diagnostics, eventually
//! current branch, open files, etc. — in the *same* shape the human sees
//! in the Problems panel. The agent never has to ask "what's broken?" or
//! call an LSP tool; the answer is already in its prompt.
//!
//! This module is deliberately small and pure: a flat list of
//! [`NamedContextBlock`]s + a deterministic composer that turns them into
//! a tagged preamble. The wire format is XML-style tags because most
//! frontier models recognize them as structured context rather than user
//! input:
//!
//! ```text
//! <workspace_diagnostics>
//! Workspace diagnostics: 2 errors, 1 warning in 2 files.
//! src/main.rs (2 errors):
//!   error 12:5 — cannot find value `x` in this scope [rust-analyzer]
//!   error 28:1 — expected `;`
//! </workspace_diagnostics>
//!
//! <user_message>
//! fix the errors
//! </user_message>
//! ```
//!
//! When the workspace has no diagnostics, no preamble is emitted at all —
//! the message goes through unchanged. That makes the integration safe to
//! enable by default in dogfood: clean repos see no behavior change.

use std::fmt::Write as _;
use std::path::PathBuf;

use chrono::{DateTime, Local};
use instant::Instant;
use wishui::{AppContext, SingletonEntity};

use crate::code::diagnostics::DiagnosticsAggregatorModel;
use crate::code::opened_files::OpenedFilesModel;
#[cfg(feature = "local_fs")]
use crate::code_review::git_status_update::{GitStatusMetadata, GitStatusUpdateModel};
use crate::persistence::model::Project;
use crate::projects::ProjectManagementModel;
use crate::terminal::history::History;

/// Default number of recently-opened files to surface to the agent.
/// Small enough to keep prompt overhead negligible, large enough to capture
/// the user's current focus across a typical multi-file session.
const RECENT_FILES_LIMIT: usize = 8;

/// Default number of recent terminal commands to surface. Small because each
/// command line can be long; ten ought to capture the user's current train of
/// thought without dominating the prompt.
const RECENT_COMMANDS_LIMIT: usize = 10;

/// Max length of a single command line in the agent context tray. Commands
/// longer than this are truncated with an ellipsis. Picked to leave room for
/// long-but-real lines (e.g. `cargo nextest run -p wish --features foo,bar`)
/// while excluding pathological pasted scripts.
const RECENT_COMMAND_MAX_LEN: usize = 200;

/// One named section of the agent context tray. The wire format is
/// `<{name}>\n{body}\n</{name}>`, so `name` must be a stable XML-safe
/// identifier that the model can pattern-match on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedContextBlock {
    pub name: &'static str,
    pub body: String,
}

/// Compose a user message with zero or more named context blocks. Pure
/// function — easy to unit-test, no `wishui` dependency. When `blocks`
/// is empty, returns `message` unchanged so prompts on clean workspaces
/// look identical to today.
pub fn compose_message_with_context(message: &str, blocks: &[NamedContextBlock]) -> String {
    if blocks.is_empty() {
        return message.to_string();
    }
    let mut out = String::new();
    for b in blocks {
        let _ = write!(
            out,
            "<{name}>\n{body}\n</{name}>\n\n",
            name = b.name,
            body = b.body.trim_end()
        );
    }
    out.push_str("<user_message>\n");
    out.push_str(message);
    out.push_str("\n</user_message>");
    out
}

/// Format a deterministic plain-text "recently opened files" block from a
/// list of repo-qualified paths already sorted by recency descending.
/// Returns `None` when `paths` is empty so callers can simply
/// `if let Some(body)` and skip the block entirely on fresh sessions.
///
/// Example output (limit=3):
///
/// ```text
/// Files the user has recently opened (3 of 5 shown):
///   src/main.rs
///   src/lib.rs
///   Cargo.toml
///   …and 2 more
/// ```
pub fn format_open_files_block(paths: &[PathBuf], limit: usize) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let shown = paths.len().min(limit);
    let mut body = if paths.len() > limit {
        format!(
            "Files the user has recently opened ({shown} of {total} shown):",
            shown = shown,
            total = paths.len()
        )
    } else {
        format!(
            "Files the user has recently opened ({} {}):",
            shown,
            if shown == 1 { "file" } else { "files" }
        )
    };
    for p in paths.iter().take(limit) {
        let _ = write!(body, "\n  {}", p.display());
    }
    if paths.len() > limit {
        let _ = write!(body, "\n  …and {} more", paths.len() - limit);
    }
    Some(body)
}

/// Test-friendly mirror of the `cfg(feature = "local_fs")`-gated
/// `GitStatusMetadata`. The pure formatter takes this value type so it can
/// be unit-tested without dragging in the entire git-watcher subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoGitSummary {
    pub path: PathBuf,
    pub branch: String,
    pub main_branch: String,
    pub files_changed: usize,
    pub total_additions: usize,
    pub total_deletions: usize,
}

/// Format the workspace's git status across every cached repo as a
/// deterministic block. `repos` must be sorted by the caller (typically by
/// path ascending). Returns `None` for an empty input so callers skip the
/// block entirely when no git metadata has been computed yet.
///
/// Format (single repo, clean):
///
/// ```text
/// Workspace git status:
///   /Users/dev/proj on `main` (main branch)
/// ```
///
/// Format (single repo, dirty):
///
/// ```text
/// Workspace git status:
///   /Users/dev/proj on `feature/foo` (main: `main`): 3 modified files (+42 −8)
/// ```
///
/// Format (multi-repo): one bullet per repo, same shape.
pub fn format_git_status_block(repos: &[RepoGitSummary]) -> Option<String> {
    if repos.is_empty() {
        return None;
    }
    let mut body = "Workspace git status:".to_string();
    for r in repos {
        body.push_str("\n  ");
        body.push_str(&r.path.display().to_string());
        body.push_str(" on `");
        body.push_str(&r.branch);
        body.push('`');
        // Only show "(main: …)" when the user isn't already on the main branch —
        // saying "on `main` (main: `main`)" is noise.
        if r.branch != r.main_branch {
            body.push_str(" (main: `");
            body.push_str(&r.main_branch);
            body.push_str("`)");
        } else {
            body.push_str(" (main branch)");
        }
        if r.files_changed == 0 {
            body.push_str(": clean");
        } else {
            let files_label = if r.files_changed == 1 {
                "modified file"
            } else {
                "modified files"
            };
            // Using a minus sign (U+2212) instead of hyphen-minus so the agent
            // doesn't confuse "-" with a path separator or option flag.
            let _ = write!(
                body,
                ": {} {} (+{} −{})",
                r.files_changed, files_label, r.total_additions, r.total_deletions
            );
        }
    }
    Some(body)
}

/// One terminal command for the `recent_terminal_commands` block. Built from
/// `HistoryEntry` for production and constructed directly in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCommand {
    pub command: String,
    /// `None` while the command is still running; `Some(n)` once complete.
    pub exit_code: Option<i32>,
}

/// Format the workspace's recent terminal activity as a deterministic block.
/// `commands` must be sorted most-recent-first by the caller. Returns `None`
/// for an empty input so callers skip the block entirely on fresh sessions.
///
/// Wish is uniquely positioned for this: VS Code and Cursor have unstructured
/// terminal output; Wish's terminal-block model means every command has a
/// known exit code, so the agent gets a high-signal "what did the user just
/// do?" view for free.
///
/// Format:
///
/// ```text
/// Recent terminal commands (3 most recent first):
///   ✗ cargo test (exit 1)
///   ✓ cargo build
///   ✓ git status
/// ```
///
/// Success markers (`✓`) elide the exit code for noise reduction; failures
/// (`✗`) and still-running (`•`) always show their status.
pub fn format_recent_commands_block(commands: &[RecentCommand], limit: usize) -> Option<String> {
    if commands.is_empty() {
        return None;
    }
    let shown = commands.len().min(limit);
    let mut body = format!("Recent terminal commands ({shown} most recent first):");
    for c in commands.iter().take(limit) {
        let marker = match c.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "•",
        };
        // Keep the line readable: collapse interior newlines and truncate.
        let single_line: String = c.command.replace('\n', " ⏎ ");
        let cmd = if single_line.chars().count() > RECENT_COMMAND_MAX_LEN {
            let truncated: String = single_line.chars().take(RECENT_COMMAND_MAX_LEN).collect();
            format!("{truncated}…")
        } else {
            single_line
        };
        let suffix = match c.exit_code {
            Some(0) => String::new(),
            Some(n) => format!(" (exit {n})"),
            None => " (still running)".to_string(),
        };
        let _ = write!(body, "\n  {marker} {cmd}{suffix}");
    }
    Some(body)
}

/// Flatten every shell-host's recent session history into a single list,
/// sorted most-recent-first. We use `completed_ts` first (the command has
/// finished; this is when the user moved on), falling back to `start_ts`
/// for still-running commands.
fn collect_recent_commands(history: &History) -> Vec<RecentCommand> {
    let mut entries: Vec<(Option<DateTime<Local>>, RecentCommand)> = history
        .iter_all_session_commands()
        .map(|entry| {
            let when = entry.completed_ts.or(entry.start_ts);
            (
                when,
                RecentCommand {
                    command: entry.command.clone(),
                    exit_code: entry.exit_code.map(|c| c.value()),
                },
            )
        })
        .collect();
    // Most recent first. None timestamps sort last (older / unknown).
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    entries.into_iter().map(|(_, c)| c).collect()
}

/// Pick the most-recently-opened project as the "active" workspace for the
/// agent prompt. Pure (no `wishui`), so it's covered by ordinary unit tests.
/// Returns `None` when the user has no projects on record (fresh install).
pub fn find_active_project(projects: &[Project]) -> Option<&Project> {
    projects.iter().max_by_key(|p| p.last_used_at())
}

/// Format the active project as a one-line context block telling the agent
/// where the user is working. The path is rendered verbatim — language
/// inference, project-type detection, and relative-path resolution are all
/// left to the model, which has enough context from the path + the other
/// blocks (file extensions, diagnostic sources) to figure it out.
pub fn format_active_project_block(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(format!("The user is working in: {path}"))
}

/// Walk every recorded `(repo, file)` pair from [`OpenedFilesModel`], join
/// each file path onto its repo root, and return them sorted by recency
/// descending. Pure list-builder so the formatter is unit-testable
/// without touching `wishui`.
fn collect_open_files(model: &OpenedFilesModel) -> Vec<PathBuf> {
    let mut all: Vec<(PathBuf, Instant)> = model
        .iter()
        .flat_map(|(repo, files)| {
            files
                .iter()
                .map(move |(file, when)| (repo.join(file), *when))
        })
        .collect();
    all.sort_by(|a, b| b.1.cmp(&a.1));
    all.into_iter().map(|(p, _)| p).collect()
}

/// Collect every live workspace context block to inject into the next
/// agent turn. Each provider is independent — adding a new block (git
/// status, last failing test, active selection) is a new branch here
/// without touching call sites.
///
/// Empty blocks are filtered out, so callers can rely on a non-empty
/// `Vec` meaning "there is something worth telling the agent."
pub fn collect_workspace_context(ctx: &AppContext) -> Vec<NamedContextBlock> {
    let mut blocks = Vec::new();

    // ── Active project (workspace identity) ──
    // Goes first so the model sees the workspace root before any path-bearing
    // diagnostics — the path resolution baseline for the rest of the prompt.
    let projects: Vec<Project> = ProjectManagementModel::as_ref(ctx)
        .all_projects()
        .cloned()
        .collect();
    if let Some(active) = find_active_project(&projects) {
        if let Some(body) = format_active_project_block(&active.path) {
            blocks.push(NamedContextBlock {
                name: "active_project",
                body,
            });
        }
    }

    // ── Git status (branch + dirty files per cached repo) ──
    // Reads cached metadata only — never creates new watchers — so this is
    // cheap on every chat turn. Skipped on WASM (no local fs).
    #[cfg(feature = "local_fs")]
    {
        let git = GitStatusUpdateModel::as_ref(ctx);
        let metadata: Vec<(PathBuf, GitStatusMetadata)> = git.cached_repo_metadata(ctx);
        let mut summaries: Vec<RepoGitSummary> = metadata
            .into_iter()
            .map(|(path, m)| RepoGitSummary {
                path,
                branch: m.current_branch_name,
                main_branch: m.main_branch_name,
                files_changed: m.stats_against_head.files_changed,
                total_additions: m.stats_against_head.total_additions,
                total_deletions: m.stats_against_head.total_deletions,
            })
            .collect();
        // Stable ordering by path so logs and prompts are deterministic.
        summaries.sort_by(|a, b| a.path.cmp(&b.path));
        if let Some(body) = format_git_status_block(&summaries) {
            blocks.push(NamedContextBlock {
                name: "git_status",
                body,
            });
        }
    }

    // ── Live LSP diagnostics ──
    let diagnostics_body = DiagnosticsAggregatorModel::as_ref(ctx).format_for_agent_context(8, 5);
    // The diagnostics formatter returns the "clean workspace" string when
    // there's nothing actionable. Skip that case — no preamble at all
    // produces a less surprising prompt than "Workspace diagnostics: clean."
    if !diagnostics_body.starts_with("Workspace diagnostics: clean") {
        blocks.push(NamedContextBlock {
            name: "workspace_diagnostics",
            body: diagnostics_body,
        });
    }

    // ── Recently opened files (user's current focus) ──
    let opened = OpenedFilesModel::as_ref(ctx);
    let paths = collect_open_files(opened);
    if let Some(body) = format_open_files_block(&paths, RECENT_FILES_LIMIT) {
        blocks.push(NamedContextBlock {
            name: "recently_opened_files",
            body,
        });
    }

    // ── Recent terminal activity (what the user just did) ──
    // This is Wish's unique advantage over VS Code / Cursor: every command
    // has a structured exit code, so the agent can see "the user just ran
    // `cargo test` and got exit 1" without reading scrollback.
    let history = History::as_ref(ctx);
    let commands = collect_recent_commands(history);
    if let Some(body) = format_recent_commands_block(&commands, RECENT_COMMANDS_LIMIT) {
        blocks.push(NamedContextBlock {
            name: "recent_terminal_commands",
            body,
        });
    }

    blocks
}

#[cfg(test)]
#[path = "agent_context_tests.rs"]
mod tests;
