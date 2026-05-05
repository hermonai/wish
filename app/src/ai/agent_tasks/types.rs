//! Pure data types for the client-side SDLC agent task surface.
//!
//! These mirror what the user sees in the Tasks panel screenshot
//! attached to the project goals — `Edited local_llm.rs +7 −1`,
//! `Ran a command, used a tool`, `Background task completed`.
//!
//! Kept in their own module so they can be:
//! - Used by both wish and wishcode (via `hermon_client` later)
//! - Unit-tested without spinning up an `AppContext`
//! - Serialized cheaply for telemetry / support bundles

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// ── Identifiers ────────────────────────────────────────────────────────

/// Stable identifier for an SDLC task.
///
/// Locally-generated and never round-tripped to a server. Wraps
/// `String` rather than `Uuid` / `Ulid` because we don't pull those
/// crates into the app's hot path; the registry generates IDs by
/// concatenating millis + a thread-local counter, same approach as
/// `LocalDriveStore::generate_id`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

impl TaskId {
    /// Construct a TaskId from any string-like value. Public so
    /// tests and remote-agent integrations can supply specific IDs.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// String view of the underlying ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Tool kinds ────────────────────────────────────────────────────────

/// The kind of tool the SDLC agent invoked to perform this task.
///
/// Drives the badge label and color in the Tasks panel: `Bash` is
/// purple, `Edit` is green, `Read` is blue, etc. The mapping lives
/// in the view layer; this enum is the single source of truth for
/// which tools exist.
///
/// Kept narrow on purpose — adding a new variant is intentional UI
/// work (new badge color, possibly new annotation shape). Tools that
/// don't justify a dedicated variant should use `Custom { name }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolKind {
    /// Shell command execution. Annotations: `CommandRun`.
    Bash,
    /// File edit (Edit / Write / NotebookEdit). Annotations: `FileEdit`.
    Edit,
    /// File read (Read / NotebookRead). Annotations: `FileRead`.
    Read,
    /// Code search (Grep / Glob). Annotations: `Search`.
    Search,
    /// Web fetch / WebSearch.
    WebFetch,
    /// MCP tool call. The `name` is the MCP tool identifier.
    Mcp { name: String },
    /// Subagent spawn (e.g. Wish Planner running a child agent).
    Subagent { agent_slug: String },
    /// Catch-all for tools that don't justify a dedicated variant.
    /// `name` is the user-facing tool label shown on the badge.
    Custom { name: String },
}

impl ToolKind {
    /// Short human-readable label shown on the badge in the Tasks
    /// panel ("Bash", "Edit", "Read"…). Single source of truth so
    /// the panel and conversation-inline rendering agree.
    pub fn badge_label(&self) -> String {
        match self {
            Self::Bash => "Bash".to_string(),
            Self::Edit => "Edit".to_string(),
            Self::Read => "Read".to_string(),
            Self::Search => "Search".to_string(),
            Self::WebFetch => "Web".to_string(),
            Self::Mcp { name } => format!("MCP/{name}"),
            Self::Subagent { agent_slug } => format!("@{agent_slug}"),
            Self::Custom { name } => name.clone(),
        }
    }
}

// ── Status state machine ──────────────────────────────────────────────

/// Lifecycle status of a task.
///
/// Transitions only flow forward (the registry enforces this):
///
/// ```text
///   Pending ──► Running ──► Completed
///                  │           ▲
///                  ├──► Failed │
///                  └──► Cancelled
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TaskStatus {
    /// Created but not yet started (e.g., approval pending, or
    /// queued behind a tool-approval modal).
    Pending,
    /// Actively running.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error. The string is a short user-facing
    /// summary; the full error trace lives in annotations.
    Failed { error: String },
    /// User-cancelled (Esc, dismiss button) before completion.
    Cancelled,
}

impl TaskStatus {
    /// Whether this status counts as "in progress" for the running list.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Running)
    }

    /// Whether this status is a terminal outcome.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed { .. } | Self::Cancelled
        )
    }

    /// Whether this status indicates failure (used by the panel to
    /// show a red border / icon).
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Validate that `next` is a legal successor to `self`.
    /// Returns `true` if the transition is allowed.
    ///
    /// Pure function — exposed for testing and for the registry's
    /// `update_status` validation.
    pub fn can_transition_to(&self, next: &TaskStatus) -> bool {
        match (self, next) {
            // Identity transitions are no-ops, allowed for idempotence.
            (a, b) if a == b => true,
            // Pending → Running | terminal
            (Self::Pending, Self::Running) => true,
            (Self::Pending, Self::Completed) => true,
            (Self::Pending, Self::Failed { .. }) => true,
            (Self::Pending, Self::Cancelled) => true,
            // Running → terminal
            (Self::Running, Self::Completed) => true,
            (Self::Running, Self::Failed { .. }) => true,
            (Self::Running, Self::Cancelled) => true,
            // Terminal states are sticky — no further transitions.
            _ => false,
        }
    }
}

// ── Annotations ───────────────────────────────────────────────────────

/// A typed annotation attached to a task. Annotations accumulate as
/// the agent makes progress — for an `Edit` task, you'll see one
/// `FileEdit` annotation; for a `Bash` task, one or more
/// `CommandRun`s; for a `Subagent`, multiple of any kind.
///
/// The view layer pattern-matches on the variant to pick rendering:
/// `FileEdit` becomes "Edited X +7 −1", `CommandRun` becomes "Ran X
/// (exit 0)", `Note` is a plain text line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskAnnotation {
    /// A file was edited. `additions` and `deletions` count lines.
    FileEdit {
        path: String,
        additions: u32,
        deletions: u32,
    },
    /// A file was read. `bytes` is the size of what was read.
    FileRead {
        path: String,
        /// Optional line range that was read.
        line_range: Option<(u32, u32)>,
    },
    /// A shell / Bash command was run. `description` is the
    /// human-readable summary the agent supplied; `exit_code` is
    /// `None` while still running.
    CommandRun {
        description: String,
        exit_code: Option<i32>,
    },
    /// A search (grep/glob) was run. `query` is the pattern,
    /// `match_count` is the number of hits.
    Search {
        query: String,
        match_count: usize,
    },
    /// A free-form note from the agent ("Background task completed",
    /// "2 shells running", etc.). Rendered as plain text in the
    /// panel.
    Note { text: String },
}

impl TaskAnnotation {
    /// Compact one-line summary suitable for a Tasks-panel chip
    /// or a conversation-inline annotation. Mirrors the screenshot's
    /// phrasing.
    pub fn one_liner(&self) -> String {
        match self {
            Self::FileEdit {
                path,
                additions,
                deletions,
            } => {
                let bare = path.rsplit('/').next().unwrap_or(path);
                format!("Edited {bare} +{additions} -{deletions}")
            }
            Self::FileRead { path, line_range } => {
                let bare = path.rsplit('/').next().unwrap_or(path);
                match line_range {
                    Some((from, to)) => format!("Read {bare} (lines {from}-{to})"),
                    None => format!("Read {bare}"),
                }
            }
            Self::CommandRun {
                description,
                exit_code,
            } => match exit_code {
                Some(0) => format!("Ran: {description}"),
                Some(code) => format!("Ran: {description} (exit {code})"),
                None => format!("Running: {description}"),
            },
            Self::Search { query, match_count } => {
                format!("Searched \"{query}\" — {match_count} matches")
            }
            Self::Note { text } => text.clone(),
        }
    }
}

// ── Task ──────────────────────────────────────────────────────────────

/// A single SDLC agent task — one tool invocation, possibly with
/// multiple annotations.
///
/// This is the unit the Tasks-panel chip in the screenshot maps to.
#[derive(Debug, Clone)]
pub struct AgentTask {
    pub id: TaskId,
    /// User-visible title shown on the chip ("Run all my tests").
    pub title: String,
    pub tool: ToolKind,
    pub status: TaskStatus,
    /// When the task was created. Drives "Xs ago" displays.
    pub started_at: Instant,
    /// When the task transitioned to a terminal status, if it has.
    pub completed_at: Option<Instant>,
    /// Typed events emitted while the task ran. The view renders
    /// these inline in the conversation as the agent works.
    pub annotations: Vec<TaskAnnotation>,
    /// Whether this task is a long-running background process the
    /// agent is monitoring (e.g., `npm run dev`). Background tasks
    /// stay in the Running list even after their initial output
    /// settles.
    pub background: bool,
    /// Free-form key-value metadata. Reserved for future extensions
    /// (telemetry tags, agent-supplied debugging info).
    pub metadata: std::collections::HashMap<String, String>,
}

impl AgentTask {
    /// How long the task has been running (or how long it ran, if
    /// terminal). Used to render "5s elapsed" badges.
    pub fn duration(&self) -> Duration {
        match self.completed_at {
            Some(end) => end.saturating_duration_since(self.started_at),
            None => self.started_at.elapsed(),
        }
    }
}
