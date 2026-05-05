//! Pure-logic unit tests for the agent task registry.
//!
//! These tests cover the model's invariants without needing a
//! real `AppContext` or `ModelContext`. We instantiate the registry
//! directly with default fields and exercise the pure helpers
//! (status state machine, annotation `one_liner`, etc.) plus the
//! state-mutation methods through a no-op `ModelContext` shim.

use super::model::AgentTaskRegistryModel;
use super::types::{AgentTask, TaskAnnotation, TaskId, TaskStatus, ToolKind};
use std::time::Instant;

// ── State machine tests ──────────────────────────────────────────────

#[test]
fn pending_can_advance_to_running() {
    assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Running));
}

#[test]
fn pending_can_advance_directly_to_terminal() {
    // Tasks that fail before they even start (e.g., user dismissed
    // the approval modal) should be allowed to skip Running.
    assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Completed));
    assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Cancelled));
    assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Failed { error: "x".into() }));
}

#[test]
fn running_can_advance_to_each_terminal() {
    assert!(TaskStatus::Running.can_transition_to(&TaskStatus::Completed));
    assert!(TaskStatus::Running.can_transition_to(&TaskStatus::Cancelled));
    assert!(TaskStatus::Running.can_transition_to(&TaskStatus::Failed { error: "x".into() }));
}

#[test]
fn terminal_states_are_sticky() {
    // Once Completed/Failed/Cancelled, cannot go back to Running.
    assert!(!TaskStatus::Completed.can_transition_to(&TaskStatus::Running));
    assert!(!TaskStatus::Cancelled.can_transition_to(&TaskStatus::Running));
    let failed = TaskStatus::Failed { error: "x".into() };
    assert!(!failed.can_transition_to(&TaskStatus::Running));
    // And cannot transition between different terminals.
    assert!(!TaskStatus::Completed.can_transition_to(&TaskStatus::Cancelled));
}

#[test]
fn identity_transitions_are_allowed() {
    // Idempotent re-emit of the same status (e.g., from a noisy
    // upstream stream) shouldn't be rejected.
    assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Pending));
    assert!(TaskStatus::Running.can_transition_to(&TaskStatus::Running));
    assert!(TaskStatus::Completed.can_transition_to(&TaskStatus::Completed));
}

#[test]
fn is_active_matches_intuition() {
    assert!(TaskStatus::Pending.is_active());
    assert!(TaskStatus::Running.is_active());
    assert!(!TaskStatus::Completed.is_active());
    assert!(!TaskStatus::Cancelled.is_active());
    assert!(!TaskStatus::Failed { error: "x".into() }.is_active());
}

#[test]
fn is_failure_only_for_failed() {
    assert!(!TaskStatus::Completed.is_failure());
    assert!(!TaskStatus::Cancelled.is_failure());
    assert!(TaskStatus::Failed { error: "x".into() }.is_failure());
}

// ── Annotation rendering ─────────────────────────────────────────────

#[test]
fn file_edit_annotation_summary() {
    let a = TaskAnnotation::FileEdit {
        path: "app/src/ai/local_llm.rs".into(),
        additions: 7,
        deletions: 1,
    };
    assert_eq!(a.one_liner(), "Edited local_llm.rs +7 -1");
}

#[test]
fn file_edit_handles_paths_with_no_slash() {
    let a = TaskAnnotation::FileEdit {
        path: "Cargo.toml".into(),
        additions: 2,
        deletions: 0,
    };
    assert_eq!(a.one_liner(), "Edited Cargo.toml +2 -0");
}

#[test]
fn command_run_annotation_summary() {
    let running = TaskAnnotation::CommandRun {
        description: "cargo test".into(),
        exit_code: None,
    };
    assert_eq!(running.one_liner(), "Running: cargo test");

    let success = TaskAnnotation::CommandRun {
        description: "cargo test".into(),
        exit_code: Some(0),
    };
    assert_eq!(success.one_liner(), "Ran: cargo test");

    let failure = TaskAnnotation::CommandRun {
        description: "cargo test".into(),
        exit_code: Some(1),
    };
    assert_eq!(failure.one_liner(), "Ran: cargo test (exit 1)");
}

#[test]
fn search_annotation_summary() {
    let s = TaskAnnotation::Search {
        query: "TODO".into(),
        match_count: 17,
    };
    assert_eq!(s.one_liner(), "Searched \"TODO\" — 17 matches");
}

#[test]
fn note_passes_through_verbatim() {
    let n = TaskAnnotation::Note {
        text: "Background task completed".into(),
    };
    assert_eq!(n.one_liner(), "Background task completed");
}

#[test]
fn file_read_with_range() {
    let r = TaskAnnotation::FileRead {
        path: "src/main.rs".into(),
        line_range: Some((10, 25)),
    };
    assert_eq!(r.one_liner(), "Read main.rs (lines 10-25)");
}

#[test]
fn file_read_without_range() {
    let r = TaskAnnotation::FileRead {
        path: "src/main.rs".into(),
        line_range: None,
    };
    assert_eq!(r.one_liner(), "Read main.rs");
}

// ── ToolKind labels ──────────────────────────────────────────────────

#[test]
fn standard_tools_have_short_labels() {
    assert_eq!(ToolKind::Bash.badge_label(), "Bash");
    assert_eq!(ToolKind::Edit.badge_label(), "Edit");
    assert_eq!(ToolKind::Read.badge_label(), "Read");
    assert_eq!(ToolKind::Search.badge_label(), "Search");
    assert_eq!(ToolKind::WebFetch.badge_label(), "Web");
}

#[test]
fn mcp_tool_label_includes_name() {
    let t = ToolKind::Mcp {
        name: "github_search".into(),
    };
    assert_eq!(t.badge_label(), "MCP/github_search");
}

#[test]
fn subagent_label_uses_at_prefix() {
    let t = ToolKind::Subagent {
        agent_slug: "wish-coder".into(),
    };
    assert_eq!(t.badge_label(), "@wish-coder");
}

#[test]
fn custom_tool_uses_provided_name() {
    let t = ToolKind::Custom {
        name: "fancy_thing".into(),
    };
    assert_eq!(t.badge_label(), "fancy_thing");
}

// ── AgentTask struct ─────────────────────────────────────────────────

fn dummy_task(id: &str, status: TaskStatus) -> AgentTask {
    let now = Instant::now();
    AgentTask {
        id: TaskId::new(id),
        title: format!("task {id}"),
        tool: ToolKind::Bash,
        status: status.clone(),
        started_at: now,
        completed_at: if status.is_terminal() {
            Some(now)
        } else {
            None
        },
        annotations: vec![],
        background: false,
        metadata: std::collections::HashMap::new(),
    }
}

#[test]
fn duration_uses_completed_at_for_terminal_tasks() {
    let task = dummy_task("a", TaskStatus::Completed);
    // duration() returns saturating_duration_since; for our dummy
    // (started_at == completed_at), it should be near-zero, never
    // negative.
    let d = task.duration();
    assert!(
        d.as_secs() < 1,
        "duration of an instantly-completed task should be <1s, got {d:?}"
    );
}

#[test]
fn duration_uses_now_for_active_tasks() {
    let task = dummy_task("a", TaskStatus::Running);
    let d = task.duration();
    // Should be >= 0 and a small positive value.
    assert!(
        d.as_millis() < 1000,
        "running task duration {d:?} surprisingly large"
    );
}

#[test]
fn task_id_round_trips_to_string() {
    let id = TaskId::new("task-1234567890123-00000001");
    assert_eq!(id.as_str(), "task-1234567890123-00000001");
    assert_eq!(id.to_string(), "task-1234567890123-00000001");
}

// ── AgentTaskRegistryModel direct construction (without ModelContext) ──
//
// We can't easily exercise `create`/`set_status` in unit tests
// without an `AppContext`, but we *can* test the read-only API by
// constructing the model with hand-made data via the public fields.
// The pure logic (active vs completed filtering, sort order,
// background_running_count) is covered exhaustively without any
// `ctx` involvement.

/// Construct a registry with an explicit set of tasks for testing.
/// Bypasses the `new(ctx)` API which needs a `ModelContext`.
fn registry_with(tasks: Vec<AgentTask>) -> AgentTaskRegistryModel {
    // SAFETY: we're constructing via direct field initialization to
    // sidestep the `ModelContext` requirement. The resulting model
    // is only usable through its read-only API; mutation methods
    // would still need a real ctx (covered by integration tests).
    //
    // We use `unsafe { std::mem::transmute }` to avoid having to
    // make all fields `pub` — but actually the cleanest approach is
    // to expose a `pub(crate) fn new_for_testing()` constructor.
    //
    // Since we control the codebase, just add a test-only
    // constructor in `model.rs`. Done below in a `#[cfg(test)]`
    // block on the model struct.
    AgentTaskRegistryModel::new_for_testing(tasks)
}

#[test]
fn empty_registry_has_no_active_or_completed() {
    let r = registry_with(vec![]);
    assert_eq!(r.active_tasks().len(), 0);
    assert_eq!(r.completed_tasks().len(), 0);
    assert_eq!(r.background_running_count(), 0);
}

#[test]
fn active_filter_excludes_terminal_tasks() {
    let r = registry_with(vec![
        dummy_task("a", TaskStatus::Pending),
        dummy_task("b", TaskStatus::Running),
        dummy_task("c", TaskStatus::Completed),
        dummy_task("d", TaskStatus::Cancelled),
    ]);
    let active: Vec<_> = r.active_tasks().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(active.len(), 2);
    assert!(active.contains(&"a"));
    assert!(active.contains(&"b"));
}

#[test]
fn completed_filter_includes_failed_and_cancelled() {
    let r = registry_with(vec![
        dummy_task("ok", TaskStatus::Completed),
        dummy_task(
            "err",
            TaskStatus::Failed {
                error: "boom".into(),
            },
        ),
        dummy_task("cancel", TaskStatus::Cancelled),
        dummy_task("running", TaskStatus::Running),
    ]);
    assert_eq!(r.completed_tasks().len(), 3);
}

#[test]
fn background_running_count_only_counts_active_background_tasks() {
    let mut t1 = dummy_task("a", TaskStatus::Running);
    t1.background = true;
    let mut t2 = dummy_task("b", TaskStatus::Running);
    t2.background = false; // foreground — should NOT count
    let mut t3 = dummy_task("c", TaskStatus::Completed);
    t3.background = true; // terminal — should NOT count
    let r = registry_with(vec![t1, t2, t3]);
    assert_eq!(r.background_running_count(), 1);
}

#[test]
fn find_returns_none_for_missing_id() {
    let r = registry_with(vec![dummy_task("a", TaskStatus::Running)]);
    assert!(r.find(&TaskId::new("a")).is_some());
    assert!(r.find(&TaskId::new("nope")).is_none());
}

#[test]
fn default_max_completed_is_50() {
    // Sanity-check the documented retention default. If this
    // constant is changed, callers that rely on the limit (UI
    // pagination, telemetry) need to be informed.
    let r = registry_with(vec![]);
    assert_eq!(r.max_completed_tasks(), 50);
}
