//! [`AgentTaskRegistryModel`] — the single source of truth for the
//! Tasks panel and the conversation-inline annotation surface.
//!
//! # Design
//!
//! Mirrors the pattern that worked for [`crate::ai::agent_registry`]:
//!
//! - One singleton, one merged list of state, granular events for
//!   selective re-renders.
//! - The view layer renders pure projections of this state — the
//!   model has no UI dependencies.
//!
//! # What lives here
//!
//! Per-tool-invocation tracking — every time the agent runs a Bash
//! command, edits a file, reads a file, etc., one `AgentTask` is
//! created. The Tasks panel and the inline conversation annotations
//! both subscribe to this model.
//!
//! # What does NOT live here
//!
//! Long-running cloud/ambient agents (the kind that survive across
//! sessions and run on the server) live in
//! [`crate::ai::ambient_agents`]. They're a different concept —
//! server-managed lifecycle, persistent identity, polling for
//! updates. This module is purely client-side and ephemeral.

use std::collections::HashMap;
use std::time::Instant;

use wishui::{Entity, ModelContext, SingletonEntity};

use super::types::{AgentTask, TaskAnnotation, TaskId, TaskStatus, ToolKind};

/// Maximum number of *terminal* tasks the registry retains in
/// memory. Older tasks are pruned in FIFO order. Keeps the panel
/// snappy on long sessions.
///
/// Tunable via [`AgentTaskRegistryModel::set_max_completed_tasks`].
const DEFAULT_MAX_COMPLETED: usize = 50;

// ── Events ────────────────────────────────────────────────────────────

/// Events emitted by [`AgentTaskRegistryModel`].
///
/// Granular by design — the Tasks panel cares about
/// `TaskCreated`/`TaskStatusChanged` to add/remove chips, while a
/// conversation-inline view cares about `AnnotationAdded` to render
/// the rolling progress lines.
#[derive(Debug, Clone)]
pub enum AgentTaskEvent {
    /// A new task was created. Payload is its ID.
    TaskCreated { id: TaskId },
    /// A task's status changed (e.g., Running → Completed).
    TaskStatusChanged { id: TaskId, new_status: TaskStatus },
    /// An annotation was appended to a task.
    AnnotationAdded {
        id: TaskId,
        annotation: TaskAnnotation,
    },
    /// A task was removed (either pruned or explicitly cleared).
    TaskRemoved { id: TaskId },
    /// Multiple tasks changed in one batch (used by `clear_completed`
    /// and `prune_to_limit` to avoid emitting N events for N
    /// removals).
    BulkChanged,
}

// ── Model ─────────────────────────────────────────────────────────────

/// Singleton registry of in-flight + recently-completed SDLC agent
/// tasks.
///
/// Construct via `add_singleton_model(AgentTaskRegistryModel::new)`
/// once at startup, then access from any view via
/// `AgentTaskRegistryModel::handle(ctx)`.
pub struct AgentTaskRegistryModel {
    /// Tasks indexed by ID, in insertion order. We use an
    /// IndexMap-like manual structure (Vec + HashMap) instead of
    /// pulling in `indexmap` because the access pattern is small
    /// and predictable.
    tasks: Vec<AgentTask>,
    /// `id → index into tasks` for O(1) lookup.
    by_id: HashMap<TaskId, usize>,
    /// Maximum number of terminal tasks retained.
    max_completed: usize,
}

impl Entity for AgentTaskRegistryModel {
    type Event = AgentTaskEvent;
}

impl SingletonEntity for AgentTaskRegistryModel {}

impl AgentTaskRegistryModel {
    /// Construct an empty registry. Wired into `lib.rs` startup.
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            tasks: Vec::new(),
            by_id: HashMap::new(),
            max_completed: DEFAULT_MAX_COMPLETED,
        }
    }

    /// Test-only constructor that lets unit tests instantiate the
    /// registry without a real `ModelContext`. Tasks are inserted
    /// in the order provided and the index is built immediately.
    ///
    /// Mutation methods still require a real context, so this
    /// constructor is only useful for exercising the read API
    /// (filtering, lookup, counts).
    #[cfg(test)]
    pub(crate) fn new_for_testing(tasks: Vec<AgentTask>) -> Self {
        let mut by_id = HashMap::new();
        for (i, task) in tasks.iter().enumerate() {
            by_id.insert(task.id.clone(), i);
        }
        Self {
            tasks,
            by_id,
            max_completed: DEFAULT_MAX_COMPLETED,
        }
    }

    // ── Read API ─────────────────────────────────────────────────────

    /// All tasks, in creation order.
    pub fn tasks(&self) -> &[AgentTask] {
        &self.tasks
    }

    /// Tasks with [`TaskStatus::is_active`] — what the panel's
    /// "Running" section renders.
    pub fn active_tasks(&self) -> Vec<&AgentTask> {
        self.tasks.iter().filter(|t| t.status.is_active()).collect()
    }

    /// Terminal tasks, newest first — what the panel's "Completed"
    /// section renders.
    pub fn completed_tasks(&self) -> Vec<&AgentTask> {
        let mut completed: Vec<&AgentTask> = self
            .tasks
            .iter()
            .filter(|t| t.status.is_terminal())
            .collect();
        completed.sort_by(|a, b| {
            b.completed_at
                .unwrap_or_else(Instant::now)
                .cmp(&a.completed_at.unwrap_or_else(Instant::now))
        });
        completed
    }

    /// Look up a task by ID.
    pub fn find(&self, id: &TaskId) -> Option<&AgentTask> {
        self.by_id.get(id).map(|&i| &self.tasks[i])
    }

    /// Number of background tasks currently running. Powers the
    /// "2 shells running" badge from the screenshot.
    pub fn background_running_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.background && t.status.is_active())
            .count()
    }

    /// The configured retention limit for terminal tasks.
    pub fn max_completed_tasks(&self) -> usize {
        self.max_completed
    }

    // ── Write API ────────────────────────────────────────────────────

    /// Create a new task. Returns the freshly-assigned `TaskId`.
    ///
    /// Initial status is [`TaskStatus::Pending`] — call
    /// [`Self::set_status`] to advance to Running.
    pub fn create(
        &mut self,
        title: impl Into<String>,
        tool: ToolKind,
        background: bool,
        ctx: &mut ModelContext<Self>,
    ) -> TaskId {
        let id = TaskId::new(generate_task_id());
        let task = AgentTask {
            id: id.clone(),
            title: title.into(),
            tool,
            status: TaskStatus::Pending,
            started_at: Instant::now(),
            completed_at: None,
            annotations: Vec::new(),
            background,
            metadata: HashMap::new(),
        };
        let index = self.tasks.len();
        self.tasks.push(task);
        self.by_id.insert(id.clone(), index);
        ctx.emit(AgentTaskEvent::TaskCreated { id: id.clone() });
        id
    }

    /// Advance a task's status. Validates the transition against
    /// [`TaskStatus::can_transition_to`]; rejects illegal transitions
    /// silently (returns `false`) so callers don't have to handle a
    /// `Result` for what's almost always a programmer error.
    ///
    /// Stamps `completed_at` on terminal transitions.
    ///
    /// Returns whether the status actually changed.
    pub fn set_status(
        &mut self,
        id: &TaskId,
        new_status: TaskStatus,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(&index) = self.by_id.get(id) else {
            return false;
        };
        let task = &mut self.tasks[index];
        if !task.status.can_transition_to(&new_status) {
            log::debug!(
                "Rejected illegal task transition for {}: {:?} → {:?}",
                id,
                task.status,
                new_status
            );
            return false;
        }
        let was_terminal = task.status.is_terminal();
        let now_terminal = new_status.is_terminal();
        let identical = task.status == new_status;
        task.status = new_status.clone();
        if !was_terminal && now_terminal {
            task.completed_at = Some(Instant::now());
        }
        if !identical {
            ctx.emit(AgentTaskEvent::TaskStatusChanged {
                id: id.clone(),
                new_status,
            });
        }
        // Whenever a task transitions to terminal, we may now exceed
        // the retention limit — prune unconditionally. Cheap when
        // the list isn't full.
        if now_terminal {
            self.prune_to_limit(ctx);
        }
        !identical
    }

    /// Append an annotation to a task.
    ///
    /// Idempotent at the storage level — if the same annotation is
    /// appended twice, both entries are kept (callers can dedupe at
    /// their layer if they want strict idempotence). This matches
    /// the screenshot semantics where every action shows up as a
    /// new line.
    pub fn add_annotation(
        &mut self,
        id: &TaskId,
        annotation: TaskAnnotation,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let Some(&index) = self.by_id.get(id) else {
            return false;
        };
        self.tasks[index].annotations.push(annotation.clone());
        ctx.emit(AgentTaskEvent::AnnotationAdded {
            id: id.clone(),
            annotation,
        });
        true
    }

    /// Set or update a metadata key-value pair.
    pub fn set_metadata(
        &mut self,
        id: &TaskId,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> bool {
        let Some(&index) = self.by_id.get(id) else {
            return false;
        };
        self.tasks[index].metadata.insert(key.into(), value.into());
        true
    }

    /// Remove a single task. Used by the "✕" dismiss button on a
    /// chip.
    pub fn remove(&mut self, id: &TaskId, ctx: &mut ModelContext<Self>) -> bool {
        let Some(&index) = self.by_id.get(id) else {
            return false;
        };
        self.tasks.remove(index);
        self.rebuild_index();
        ctx.emit(AgentTaskEvent::TaskRemoved { id: id.clone() });
        true
    }

    /// Clear all terminal (completed/failed/cancelled) tasks.
    /// Active tasks are kept. Powers the "Clear completed" button.
    pub fn clear_completed(&mut self, ctx: &mut ModelContext<Self>) {
        let before = self.tasks.len();
        self.tasks.retain(|t| !t.status.is_terminal());
        let after = self.tasks.len();
        if after != before {
            self.rebuild_index();
            ctx.emit(AgentTaskEvent::BulkChanged);
        }
    }

    /// Set the maximum number of terminal tasks retained.
    /// Triggers a prune if the new limit is below the current count.
    pub fn set_max_completed_tasks(&mut self, max: usize, ctx: &mut ModelContext<Self>) {
        self.max_completed = max;
        self.prune_to_limit(ctx);
    }

    /// Prune oldest terminal tasks to stay under the retention limit.
    /// Active tasks are never pruned regardless of count.
    fn prune_to_limit(&mut self, ctx: &mut ModelContext<Self>) {
        let mut terminal_indices: Vec<usize> = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.status.is_terminal())
            .map(|(i, _)| i)
            .collect();
        if terminal_indices.len() <= self.max_completed {
            return;
        }
        // Sort terminal indices by `completed_at` ascending so we
        // remove the oldest first.
        terminal_indices.sort_by_key(|&i| self.tasks[i].completed_at.unwrap_or_else(Instant::now));
        let drop_count = terminal_indices.len() - self.max_completed;
        let mut drop_set: std::collections::HashSet<usize> =
            terminal_indices.iter().take(drop_count).copied().collect();
        // Walk the vec in reverse and drop matching indices.
        let mut i = self.tasks.len();
        while i > 0 {
            i -= 1;
            if drop_set.remove(&i) {
                self.tasks.remove(i);
            }
        }
        self.rebuild_index();
        ctx.emit(AgentTaskEvent::BulkChanged);
    }

    /// Rebuild the `by_id` index after any operation that shifts
    /// vector indices (remove / clear / prune). O(n) — acceptable
    /// for the small task counts involved.
    fn rebuild_index(&mut self) {
        self.by_id.clear();
        for (i, task) in self.tasks.iter().enumerate() {
            self.by_id.insert(task.id.clone(), i);
        }
    }
}

// ── ID generation ─────────────────────────────────────────────────────

/// Generate a stable-ish unique task ID without pulling in `ulid` /
/// `uuid`. Format: `task-<unix-millis>-<thread-local counter>`.
///
/// Collision-safe within a process: the thread-local counter
/// monotonically increases. Across-restart uniqueness isn't needed
/// because tasks are ephemeral (not persisted).
fn generate_task_id() -> String {
    use std::cell::Cell;
    use std::time::{SystemTime, UNIX_EPOCH};

    thread_local! {
        static COUNTER: Cell<u64> = Cell::new(0);
    }
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = COUNTER.with(|c| {
        let next = c.get().wrapping_add(1);
        c.set(next);
        next
    });
    format!("task-{millis:013}-{n:08}")
}
