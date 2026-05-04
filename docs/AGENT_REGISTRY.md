# Agent Registry

The Agent Registry (`crate::ai::agent_registry`) is the **single source of
truth for "what agents are available to invoke right now"** in Wish.

## Why a separate module

Multiple UI features need to list, search, and pick agents:
- Agent management panel (CRUD)
- Conversation panel (start a chat with `@some-agent`)
- Command palette (jump to agent by slug)
- Settings → Built-in Agents

Without a shared registry, each of those features would re-implement
fetch/cache/fallback logic. The registry centralizes that: one model,
one set of events, one consistent view of the world.

## Two sources, one list

```
┌──────────────────────────┐    ┌──────────────────────────┐
│  Hermon backend          │    │  Built-in SDLC defs      │
│  /v1/agents              │    │  hermon_client::sdlc     │
│  (network, may fail)     │    │  (always available)      │
└────────────┬─────────────┘    └─────────────┬────────────┘
             │                                │
             │           merge()              │
             └───────────────┬────────────────┘
                             ▼
              ┌──────────────────────────────┐
              │   AgentRegistryModel         │
              │   Vec<RegistryEntry>         │
              │   HashMap<slug, idx>         │
              │   HashMap<id, idx>           │
              └──────────────────────────────┘
```

### Merge rules

1. Hermon agents come first (preserves API order — usually
   created-time descending).
2. Built-in agents follow, **except** any built-in whose slug is
   already claimed by a Hermon agent.
3. This means operators can override a default by registering a
   server-side agent with the same slug — no client redeploy needed.

## Failure modes

| Situation | Behaviour |
|-----------|-----------|
| Hermon unconfigured (no API key) | Status stays `Idle`. Built-ins are the only entries. |
| Hermon network error | Status → `Failed`. **Previous list retained.** Built-ins always present as the floor. |
| Hermon returns empty | Built-ins are still listed (Hermon's empty list doesn't override built-ins). |
| Concurrent refresh requests | Coalesced — second caller is a no-op while one is in flight. |
| Server misreports `has_more` | Pagination capped at 50 pages with a warning log. |

## Source tagging

Every entry carries an `AgentSource` tag (`Hermon` or `BuiltIn`). UI uses
this to:
- Show different affordances (built-ins are read-only — no edit/delete).
- Visually distinguish system vs. user-created agents.
- Decide which "create" flow to invoke (clone-from-builtin vs. fresh).

## Lookups

```rust
let registry = AgentRegistryModel::handle(ctx);

// Iterate everything
for entry in registry.as_ref(ctx).entries() { ... }

// Resolve a user-typed @slug from the command palette
let coder = registry.as_ref(ctx).find_by_slug("wish-coder");

// Find by ID (works for both real Hermon IDs and "builtin:wish-coder")
let entry = registry.as_ref(ctx).find_by_id(some_id);

// Filter
let sdlc_agents = registry.as_ref(ctx).by_type(&AgentType::Sdlc);
```

## Events

Subscribe to `AgentRegistryEvent` for reactivity:

| Event | Fires when |
|-------|------------|
| `AgentsChanged` | Effective list changed (new entries, source priority shift, manual edit) |
| `StatusChanged` | `RegistryStatus` transitions (Refreshing ↔ Loaded/Failed) |

The split lets a status indicator (loading spinner, error toast) render
without redrawing the list, and vice versa.

## Refresh policy

Currently the registry refreshes once at app startup. Future work:
- Refresh on auth change (user log-in/log-out).
- Periodic refresh (e.g., every 5 minutes) when the agent panel is
  visible.
- Manual refresh from a "Reload agents" affordance in the panel.

The model already has a public `refresh(ctx)` method — adding any of
the triggers above is a small change.

## Thread safety

`AgentRegistryModel` is a wishui `SingletonEntity`. All mutation happens
through `ModelContext`, which serializes back onto the model thread.
The async fetch (`fetch_hermon_agents`) is `Send + 'static` and only
returns a `Result<Vec<Agent>, String>` to the model thread for
application — there are no shared mutable references.

## Testing

- **Pure logic** (`merge`, ID conversion) tested without an `AppContext`.
- **Built-in conversion** verified end-to-end: every SDLC slug appears
  after merging with no Hermon agents.
- **Override priority**: a Hermon agent with the same slug as a
  built-in correctly displaces the built-in.

Async refresh paths are tested separately against a mocked
`HermonClient` (planned for next phase).

## Where the registry plugs in

| Caller | Status | Purpose |
|--------|--------|---------|
| Settings → "Built-in Agents" page | **Built** | List all agents (Hermon + 10 SDLC built-ins) with descriptions, model, tools, capabilities |
| Command palette `@<slug>` | Planned | Resolve slug → start invocation |
| Conversation panel agent picker | Planned | Drop-down of available agents |
| Agent management view | Planned | CRUD over Hermon-sourced entries; read-only view of built-ins |

## Settings page integration (current)

The "Built-in Agents" page (`crate::settings_view::builtin_agents_page`)
is the first concrete consumer of the registry. It demonstrates the
intended consumption pattern:

1. **Construction** — view subscribes to `AgentRegistryEvent` once in
   `BuiltInAgentsPageView::new` and calls `ctx.notify()` on any event.
   This is intentionally coarse: any registry change re-renders.
   The view *also* subscribes to `BuiltInAgentsUiStateEvent` (UI-only
   state) so card-expansion changes also trigger re-renders.
2. **Render** — `SettingsWidget::render` reads `entries()` and
   `status()` directly from `registry.as_ref(app)`. No state caching
   in the widget — registry is the single source of truth.
3. **Refresh** — the "Refresh" button dispatches
   `WorkspaceAction::RefreshAgentRegistry`. The workspace handler
   forwards to `registry.update(...).refresh(ctx)`. Click handlers can't
   call models directly because they receive `EventContext` rather than
   `ViewContext`.
4. **Card click → expand** — clicking an agent card dispatches
   `WorkspaceAction::ToggleAgentDetails { slug }`. The handler updates
   `BuiltInAgentsUiState` (a separate singleton), and the page re-renders
   to show the system prompt, instructions, parameters, metadata, and
   timestamps for that agent.
5. **Copy slug** — each card has a "Copy slug" button that dispatches
   `WorkspaceAction::CopyAgentSlug { slug }`. Slugs are the stable
   user-facing reference (e.g., `@wish-coder` in chat once the
   command-palette integration ships).
6. **Search filter** — a single-line text input at the top of the
   page filters which cards are visible. Each keystroke dispatches
   `WorkspaceAction::SetAgentSearchQuery { query }`, which updates
   `BuiltInAgentsUiState`. The page reads the query during render
   and applies `filter::matches_query()` (a pure predicate, fully
   unit-tested) to the entry list. Match is case-insensitive
   substring across name, slug, type label, description, tool IDs,
   capabilities, and source label; multi-word queries require every
   token to match somewhere. The status row switches to
   "M of N agents match" when the filter is active. The
   `matches_query` predicate is reusable — the future picker modal
   and command palette will share the same implementation.

## Why UI state is its own singleton

`BuiltInAgentsUiState` (currently: which card is expanded) lives in a
*separate* singleton from `AgentRegistryModel`. This is deliberate:

- The registry is the source of truth for *what agents exist*. Its
  events (`AgentsChanged`, `StatusChanged`) describe data changes that
  every consumer cares about.
- The UI state is the source of truth for *how the user is currently
  looking at them*. Its events (`ExpansionChanged`) only matter to the
  Built-in Agents page.

Mixing them would mean any view subscribed to `AgentRegistryModel`
(e.g., a future conversation panel agent picker) would receive
expand/collapse events it has no reason to care about, increasing
re-renders. Splitting them keeps each model's event stream tight.
