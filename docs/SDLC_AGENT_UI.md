# SDLC Agent UI Architecture

This document describes the **client-side surfaces** that visualize
SDLC agent work in Wish (and, by reuse of the wire types, in
wishcode). The screenshots in the project goals show what we're
building: a **Tasks panel** with running/completed sections,
**inline annotations** in the conversation (`Edited X +7 -1`,
`Ran a command`), and a **header badge** for background processes.

## Goal

Match — and ultimately exceed — the Claude Code task UX:

- Every tool the agent invokes is visible and dismissable
- File edits show line counts inline
- Bash commands show their description and exit code
- Long-running shells/builds are tracked separately and don't
  disappear when their initial output settles
- The user can clear completed tasks, dismiss individual chips,
  and (future) re-run a task from a chip

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Agent runtime  (hermon_client::ai stream  OR  local LLM)        │
│        │                                                          │
│        │  emits: ToolUse, ToolResult, AgentStreamEvent::ToolCall │
│        ▼                                                          │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │  AgentTaskRegistryModel  (singleton, this turn)         │     │
│  │  ─────────────────────────                              │     │
│  │  • create(title, ToolKind, background)                  │     │
│  │  • set_status(id, TaskStatus)                           │     │
│  │  • add_annotation(id, TaskAnnotation)                   │     │
│  │  • clear_completed()                                    │     │
│  │                                                         │     │
│  │  Events: TaskCreated · TaskStatusChanged ·             │     │
│  │          AnnotationAdded · TaskRemoved · BulkChanged    │     │
│  └─────────────────────────────────────────────────────────┘     │
│        │                  │                  │                   │
│        ▼                  ▼                  ▼                   │
│  ┌───────────┐    ┌──────────────┐    ┌──────────────────┐       │
│  │  Tasks    │    │  Inline      │    │  Workspace       │       │
│  │  panel    │    │  annotations │    │  header badge    │       │
│  │  view     │    │  in conv.    │    │  ("2 shells")    │       │
│  └───────────┘    └──────────────┘    └──────────────────┘       │
└──────────────────────────────────────────────────────────────────┘
```

Same architecture as `AgentRegistryModel` → `BuiltInAgentsPageView`:
**one model, multiple views, granular events**. Each surface
subscribes to only the events it cares about, so the conversation
view doesn't re-render when a chip is dismissed in the panel.

## Data model (built — `crate::ai::agent_tasks`)

### `TaskStatus` state machine

```text
                ┌──────► Completed
                │
   Pending ──► Running ──► Failed { error }
                │
                └──────► Cancelled
```

Pending → Running is the typical happy path; Pending → terminal is
allowed for tasks that fail before they start (e.g., user dismisses
the approval modal). Terminal states are sticky — the registry
rejects illegal transitions silently and logs at `debug`.

### `ToolKind`

| Variant | Badge label | Used by |
|---|---|---|
| `Bash` | `Bash` | Shell command execution |
| `Edit` | `Edit` | File edit (Edit / Write / NotebookEdit tools) |
| `Read` | `Read` | File read (Read / NotebookRead) |
| `Search` | `Search` | Code search (Grep / Glob) |
| `WebFetch` | `Web` | WebFetch / WebSearch |
| `Mcp { name }` | `MCP/<name>` | Any MCP tool call |
| `Subagent { agent_slug }` | `@<slug>` | Subagent spawn |
| `Custom { name }` | `<name>` | Catch-all |

The badge label is the single source of truth — both the panel and
the inline annotations use `tool.badge_label()`.

### `TaskAnnotation` variants

| Variant | One-liner format |
|---|---|
| `FileEdit { path, +N, -M }` | `Edited <basename> +N -M` |
| `FileRead { path, line_range }` | `Read <basename> (lines a-b)` |
| `CommandRun { description, exit_code }` | `Ran: <desc>` / `Running: <desc>` / `Ran: <desc> (exit N)` |
| `Search { query, match_count }` | `Searched "<q>" — N matches` |
| `Note { text }` | `<text>` |

The `one_liner()` method is the single source of truth for these
phrasings; both surfaces use it. Want to change wording? Edit one
function.

### Retention

Terminal tasks are pruned in FIFO order to stay under
`max_completed_tasks` (default **50**). Active tasks are never
pruned. The limit is tunable per-model so headless agents
(integration tests, batch runs) can lift it without forking the
type.

## Views (deferred to follow-up turns)

### 1. `TasksPanelView` — the right-side sidebar

A `ViewHandle<TasksPanelView>` registered as a workspace-level
panel (next to `code_review_pane_view`, etc.).

```rust
struct TasksPanelView {
    pane_configuration: ModelHandle<PaneConfiguration>,
    focus_handle: Option<PaneFocusHandle>,
    // Mouse states: per-task dismiss button, "Clear completed" button
    dismiss_states: RefCell<HashMap<TaskId, MouseStateHandle>>,
    clear_completed_state: MouseStateHandle,
}

impl TasksPanelView {
    fn new(ctx) -> Self {
        // Subscribe to AgentTaskEvent → ctx.notify()
        let registry = AgentTaskRegistryModel::handle(ctx);
        ctx.subscribe_to_model(&registry, |_, _, _, ctx| ctx.notify());
        ...
    }
}

impl View for TasksPanelView {
    fn render(&self, app) -> Box<dyn Element> {
        let registry = AgentTaskRegistryModel::handle(app).as_ref(app);

        // Two sections: "Running" (active) + "Completed"
        // Each chip renders:
        //   [icon] <title>           [Tool badge]   [✕]
        //          <annotations.last().one_liner()>
        ...
    }
}
```

Estimated effort: 2-3 hours.

### 2. Inline conversation annotations

In the conversation message renderer, when a message contains a
tool-use block, look up the corresponding task in
`AgentTaskRegistryModel` and render its annotations as a small
indented list under the assistant's message. Subscribe only to
`AnnotationAdded` events for tasks visible in the current scroll
window, so off-screen tasks don't trigger re-renders.

Estimated effort: 2 hours.

### 3. Header badge — `2 shells running`

A small chip in the workspace header that shows
`background_running_count()`. Single-line, click navigates focus to
the Tasks panel.

```rust
let count = AgentTaskRegistryModel::handle(app)
    .as_ref(app)
    .background_running_count();
if count > 0 {
    render_chip(format!("{count} shell{}", if count == 1 { "" } else { "s" }));
}
```

Estimated effort: 30 minutes.

## Wiring the agent runtime

The agent runtime emits events as it makes progress. Each path needs
a small adapter that translates runtime events into registry
operations:

### Hermon stream adapter

```rust
// app/src/ai/agent_tasks/hermon_adapter.rs (future)
fn handle_hermon_stream_event(
    event: AgentStreamEvent,
    registry: &mut AgentTaskRegistryModel,
    ctx: &mut ModelContext<...>,
) {
    match event {
        AgentStreamEvent::ToolCallStart { tool_id, name, .. } => {
            let kind = ToolKind::from_hermon(&name);
            let id = registry.create(name, kind, false, ctx);
            // Stash hermon's tool_id → our TaskId for later result mapping
            ...
        }
        AgentStreamEvent::ToolCallChunk { tool_id, kind, .. } => {
            // Append a typed annotation based on `kind`
            registry.add_annotation(...);
        }
        AgentStreamEvent::ToolCallEnd { tool_id, success, .. } => {
            registry.set_status(
                &task_id_for(tool_id),
                if success { TaskStatus::Completed } else { TaskStatus::Failed { ... } },
                ctx,
            );
        }
        ...
    }
}
```

### Local LLM adapter

For Ollama / OpenAI-compatible providers (see
`crate::ai::local_llm`), tool use comes back as JSON in the
assistant message. The adapter parses that and feeds the same
registry operations.

## Cross-client reuse: wishcode

The data types in `crate::ai::agent_tasks::types` are pure data
(serde-friendly, no UI deps). Wishcode (Electron + TypeScript) gets
the same surface by:

1. Generating TS types from the Rust definitions via `ts-rs` /
   `tsify` (already in workspace `Cargo.toml` for other surfaces)
2. Implementing a parallel `agent-tasks-store` package in
   wishcode that exposes the same status state machine in TS
3. Both clients consume the same `hermon_client::types::ai`
   stream events, so the conversion logic is identical

The pure-data tests in `tests.rs` (state machine validity,
annotation rendering) are **wire-format agreements** — both
clients should pass equivalent tests.

## Test coverage (this turn)

23 unit tests, all pure-logic, no `AppContext` needed:

- **6** state-machine transition tests (Pending ↔ Running ↔
  Completed/Failed/Cancelled, terminal stickiness, identity, is_active,
  is_failure)
- **8** annotation rendering tests (FileEdit basename extraction,
  CommandRun success/running/failure formats, Search, Note,
  FileRead with/without range)
- **4** ToolKind label tests (standard, MCP, Subagent, Custom)
- **5** AgentTask tests (duration on terminal vs running, ID
  round-trip, registry empty/active/completed filters,
  background_running_count)

## Cross-references

- `docs/AGENT_REGISTRY.md` — the agent catalog (already shipped)
- `docs/LOCAL_DEV.md` — pure-local mode setup (Ollama, local Hermon)
- `docs/LOCAL_MODE_ARCHITECTURE.md` — local-LLM and local-Drive
  scaffolding
- `crates/hermon_client/src/types/ai.rs` — wire-level
  `AgentStreamEvent` enum that the runtime emits
