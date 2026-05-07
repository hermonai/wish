# Wish Chat Integration Path

This document explains how to wire the **chat view** (the next
session's primary deliverable) into the foundation that's already
shipped. Everything below is plumbing — the data model, adapters,
and registries are ready. The remaining work is connecting them.

## What's already in place

| Piece | Location | Status |
|-------|----------|--------|
| Conversation data types | `app/src/ai/wish_conversation/types.rs` | ✅ shipped, 14 tests |
| Stream state machine (`InFlight::apply`) | `app/src/ai/wish_conversation/model.rs` | ✅ shipped, 6 tests |
| Conversation singleton | `app/src/ai/wish_conversation/model.rs` | ✅ shipped, 5 tests |
| `ConversationAdapter` trait | `app/src/ai/wish_conversation/adapter.rs` | ✅ shipped |
| `StubAdapter` (canned-response) | `app/src/ai/wish_conversation/adapter.rs` | ✅ shipped, 4 tests |
| **`LocalLlmAdapter`** (Ollama / OpenAI-compat) | `app/src/ai/wish_conversation/local_llm_adapter.rs` | ✅ shipped, 17 tests |
| `AgentTaskRegistryModel` (tool-use chips) | `app/src/ai/agent_tasks/` | ✅ shipped, 27 tests |
| `AgentRegistryModel` (built-in agents) | `app/src/ai/agent_registry/` | ✅ shipped, 30 tests |

## What's NOT in place yet

| Piece | Location (suggested) | Effort | Notes |
|-------|----------------------|--------|-------|
| **`HermonAdapter`** | `wish_conversation/hermon_adapter.rs` | ~150 LOC | Wraps `hermon_client.conversations.send_message` SSE stream. |
| **`WishChatView` settings page** | `settings_view/wish_chat_page.rs` | ~250 LOC | Renders `ConversationManagerModel` projection. Same pattern as `BuiltInAgentsPageView` / `SdlcTasksPageView`. |
| **Workspace actions** | `workspace/action.rs` | ~30 LOC | `SendChatMessage { conversation_id, content }`, `CreateConversation { agent_slug }`, `ApplyChunk { conversation_id, chunk }` |
| **Active adapter selection** | `workspace/view.rs` handler | ~50 LOC | Pick `HermonAdapter` if logged in, else `LocalLlmAdapter` if Ollama configured, else `StubAdapter`. |

## Wiring the chat view (recipe for next session)

### 1. Workspace actions

Add to `app/src/workspace/action.rs`:

```rust
pub enum WorkspaceAction {
    // ...existing variants...

    /// Create a new conversation with the given primary agent.
    CreateConversation { agent_slug: String },

    /// Append a user message and dispatch the active adapter.
    SendChatMessage {
        conversation_id: String,
        content: String,
    },

    /// Apply a streaming chunk (called from the adapter sink).
    ApplyChatChunk {
        conversation_id: String,
        chunk_json: String, // serialized StreamChunk
    },
}
```

Add to `app/src/workspace/view.rs::Workspace::view`:

```rust
CreateConversation { agent_slug } => {
    let mgr = ConversationManagerModel::handle(ctx);
    let agent_slug = agent_slug.clone();
    mgr.update(ctx, |m, ctx| {
        m.create(agent_slug, ctx);
    });
}

SendChatMessage { conversation_id, content } => {
    let mgr = ConversationManagerModel::handle(ctx);
    let id = ConversationId::new(conversation_id.clone());
    let content = content.clone();
    mgr.update(ctx, move |m, ctx| {
        m.append_user(&id, content.clone(), ctx);
        // Get a snapshot of the conversation for the adapter.
        let snapshot = m.get(&id).cloned();
        if let Some(snapshot) = snapshot {
            let adapter = pick_active_adapter(ctx);
            // Dispatch the adapter on a background thread; the
            // sink converts each chunk into ApplyChatChunk
            // dispatched via a thread-safe action channel.
            let id_for_sink = id.clone();
            adapter.send(
                &snapshot,
                &content,
                Box::new(move |chunk| {
                    let json = serde_json::to_string(&chunk).unwrap_or_default();
                    dispatch_thread_safe(WorkspaceAction::ApplyChatChunk {
                        conversation_id: id_for_sink.0.clone(),
                        chunk_json: json,
                    });
                }),
            );
        }
    });
}

ApplyChatChunk { conversation_id, chunk_json } => {
    let mgr = ConversationManagerModel::handle(ctx);
    let id = ConversationId::new(conversation_id.clone());
    let chunk: StreamChunk = serde_json::from_str(chunk_json).unwrap_or_else(|e| {
        StreamChunk::Error { message: format!("chunk decode failed: {e}") }
    });
    mgr.update(ctx, |m, ctx| {
        m.apply_chunk(&id, chunk, ctx);
    });
}
```

### 2. Pick the active adapter

```rust
fn pick_active_adapter(ctx: &AppContext) -> Box<dyn ConversationAdapter> {
    // 1. If Hermon is configured + user logged in, use it.
    if let Some(client) = HermonServiceModel::handle(ctx)
        .as_ref(ctx)
        .client()
        .cloned()
    {
        if AuthManager::handle(ctx).as_ref(ctx).is_logged_in() {
            return Box::new(HermonAdapter::new(client));
        }
    }
    // 2. If a local LLM provider is configured, use it.
    if let Some(local_config) = AISettings::handle(ctx)
        .as_ref(ctx)
        .local_llm_config()
        .cloned()
    {
        return Box::new(LocalLlmAdapter::new(local_config));
    }
    // 3. Fallback: stub.
    Box::new(StubAdapter::new())
}
```

### 3. The chat page view

Mirror `BuiltInAgentsPageView`:

```rust
pub struct WishChatPageView {
    page: PageType<Self>,
    composer: ViewHandle<EditorView>,
    composer_button_state: MouseStateHandle,
    new_conversation_button_state: MouseStateHandle,
}

impl WishChatPageView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let mgr = ConversationManagerModel::handle(ctx);
        ctx.subscribe_to_model(&mgr, |_, _, _, ctx| ctx.notify());

        // Composer editor (single line for now; multi-line later)
        let composer = build_composer_editor(ctx);

        Self { page, composer, /* ... */ }
    }
}
```

Render layout:

```text
┌────────────────────────────────────────────────────────┐
│ Conversation list (left, 200px)  │  Active conversation │
│ ┌────────────────────────────┐   │ ┌──────────────────┐│
│ │ + New conversation         │   │ │ Title bar        ││
│ ├────────────────────────────┤   │ │ "Running on stub"││
│ │ Refactor local_llm.rs   ▶  │   │ ├──────────────────┤│
│ │ Add unit tests             │   │ │                  ││
│ │ Update README              │   │ │  ▸ User: hello   ││
│ │ ...                        │   │ │  ▸ Assistant: hi ││
│ └────────────────────────────┘   │ │  ▸ ToolUse chip  ││
│                                  │ │                  ││
│                                  │ ├──────────────────┤│
│                                  │ │ [composer ↵]     ││
│                                  │ └──────────────────┘│
└────────────────────────────────────────────────────────┘
```

### 4. Tool-use → AgentTaskRegistry

When the assistant emits a `ToolUse` block, the adapter ALSO
creates a corresponding `AgentTask` in
`AgentTaskRegistryModel`. This is what powers the SDLC Tasks
panel — it's already shipped, just needs the adapter side of
the wiring:

```rust
// Inside HermonAdapter::send (when a ToolCallStart event arrives):
let task_id = registry.update(ctx, |r, ctx| {
    r.create(tool_name.clone(), ToolKind::from_name(&tool_name), false, ctx)
});
sink(StreamChunk::ToolUse {
    task_id,
    tool_name,
    input_json,
});
// ...later, on ToolCallEnd:
registry.update(ctx, |r, ctx| {
    r.set_status(&task_id, TaskStatus::Completed, ctx);
});
```

## Architecture invariants

These have been baked into the model and **must not be violated**
when adding the chat view:

1. **One adapter contract**: `send` MUST emit exactly one `Done`
   or `Error` chunk at the end. The InFlight state machine
   relies on this.
2. **Conversations are append-only**: turns are never edited or
   removed (only the in-flight buffer can be cleared). This keeps
   undo/replay tractable.
3. **The chat view never touches HTTP directly** — it dispatches
   actions; adapters do the I/O. This keeps the view pure-render
   and trivially testable.
4. **Wire types are shared**: `ChatCompletionRequest`,
   `ChatMessage`, etc. come from `crate::ai::local_llm`, not
   re-declared here. Schema drift = compile error.
5. **`AgentTask::id` is the cross-reference** between the chat
   view's `MessageBlock::ToolUse` and the Tasks panel. Both
   surfaces look up the same ID from `AgentTaskRegistryModel`.

## Testing strategy for the next session

- **`HermonAdapter`**: pure-function tests for the
  AgentStreamEvent → StreamChunk translation; integration test
  against a mock SSE server (use `httpmock`).
- **Active-adapter selection**: pure-function test of
  `pick_active_adapter` given (auth state, local config) inputs.
- **Chat page**: pure-logic helpers (turn-renderer phrasing,
  composer-disabled-while-streaming) directly tested. Visual
  smoke tests deferred until preview-tools support headless
  Wish.

## What "good enough" looks like

The next session ships a working chat surface if:

1. `cargo run --bin wish-oss` opens with the agent registry
   populated, the Tasks panel empty.
2. Settings → Wish Chat opens a chat page with one default
   conversation against `wish-coder`.
3. With Ollama running locally, typing "hello" + Enter produces
   a streamed response.
4. Without Ollama, the same flow shows a graceful error in the
   chat header: "Local LLM not reachable at
   http://localhost:11434/v1 — start Ollama or sign in to
   Hermon."
5. With Hermon login configured, the same flow uses the cloud
   adapter and tool-use chips populate the Tasks panel.
