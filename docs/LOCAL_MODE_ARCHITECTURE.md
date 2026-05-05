# Local-mode architecture

This document describes the **scaffolding** that's been built to make
Wish work end-to-end without a Hermon backend, and the remaining
integration work to fully wire it into the UI.

## What's already built (this session)

### `app/src/drive/local_store.rs` — Local filesystem Drive

A complete, **fully unit-tested** filesystem-backed store at
`~/.wish/drive/`:

```rust
let store = LocalDriveStore::at_default_location()?;

// CRUD
let obj = store.create(
    "My Workflow".into(),
    DriveObjectType::Workflow,
    None,           // parent_id (None = root)
    None,           // metadata
)?;
store.write_content(&obj.id, b"#!/bin/sh\necho hello")?;
let bytes = store.read_content(&obj.id)?;
store.delete(&obj.id)?;
let all = store.list(None)?;
```

**Layout**

```text
~/.wish/drive/
├── README.md                       # human-readable explainer (auto-created)
├── index.json                      # full object index for fast list()
└── objects/
    ├── <id>.json                   # one metadata file per object
    └── <id>.content                # optional content blob
```

**Wire-type compatibility:** the store accepts and returns
`hermon_client::types::drive::DriveObject` directly, so a future
Hermon-vs-local switch is a single function-pointer swap rather
than a full translation layer.

**ID namespacing:** local IDs are prefixed `local:` and can never
collide with Hermon-issued IDs.

**Tests:** 13 unit tests covering create/read/update/delete, parent
filtering, sort order, ID uniqueness across rapid creates, and
ISO-8601 timestamp formatting.

### `app/src/ai/local_llm.rs` — Local LLM provider config + wire types

A typed configuration surface for OpenAI-compatible local endpoints
(Ollama, llama.cpp, LM Studio, vllm):

```rust
LocalLlmProviderConfig::OpenAiCompatible {
    base_url: "http://localhost:11434/v1".into(),
    api_key: "ollama".into(),
    default_model: "llama3.2:3b".into(),
    timeout_secs: None,            // defaults to 60s
}
```

**Wire types**: `ChatMessage`, `ChatCompletionRequest`,
`ChatCompletionResponse`, `ChatCompletionChoice` — minimal subset of
the OpenAI schema, deserialization-tolerant of extra fields (so
Ollama's `usage` block etc. don't break parsing).

**Health probe**: `LocalLlmHealth { Ok | BadResponse | Unreachable }`
lets the Settings → AI page surface a connectivity dot per
configured provider.

**Privacy:** `api_key` is `serde(skip_serializing)` so it never lands
in debug-log dumps.

**Tests:** 8 unit tests covering type labels, default models, custom
timeouts, JSON / TOML round-tripping, and the api-key redaction
invariant.

## What's NOT built yet (next session)

The scaffolding above is *complete and tested in isolation*, but the
glue that connects it to the rest of Wish lives in two follow-up
tasks:

### 1. UI integration: route Drive panel through `LocalDriveStore`

The current Drive panel (`app/src/drive/panel.rs`) talks to the
network via `cloud_object` types. To make it work in local mode:

1. Define a `DriveBackend` trait that exposes the small surface the
   panel actually uses (probably `list_root`, `list_children`,
   `get`, `delete`). Pure functional methods, no
   network-specific signatures.
2. Implement `DriveBackend` for `LocalDriveStore` (already-tested
   methods map 1:1).
3. Implement `DriveBackend` for the existing `cloud_object` client.
4. The panel constructor picks one based on
   `HermonServiceModel::handle(ctx).as_ref(ctx).client().is_some()`
   — falls back to local when no Hermon client is configured.
5. Add a small "🟢 Local mode" badge in the panel header so users
   know which backend they're hitting.

Estimated effort: 2-3 focused hours, mostly typing and tests for the
trait abstraction. The risky part (ID compatibility, on-disk format)
is already done.

### 2. Agent invocation: route via `LocalLlmProviderConfig`

The Built-in SDLC agents (`hermon_client::types::sdlc`) are
discoverable in the registry but can't yet *be invoked* in local
mode. To make them callable:

1. Define a `LlmInvoker` trait that takes a chat-style request and
   returns a stream of completion events (matching `hermon_client`'s
   `AgentStreamEvent` shape so callers don't care which backend
   responds).
2. Implement `LlmInvoker` for `LocalLlmProviderConfig::OpenAiCompatible`
   using the wire types in `local_llm.rs`. Streaming via
   `text/event-stream` parsing of `data: {...}` lines.
3. Implement `LlmInvoker` for the Hermon `ai` namespace (already
   has `stream` on `client.ai`).
4. The agent runtime picks one based on the same
   `HermonServiceModel` / settings.toml configuration check used by
   Drive.
5. Surface a "running on local llama3.2:3b" indicator in the
   conversation header.

Estimated effort: 4-6 focused hours. The Hermon side is wired; the
Ollama side is well-spec'd; the streaming parser is the most
involved single piece.

## Design principles

These guided the scaffolding above, and should guide the integration
work:

1. **Wire-type compatibility over translation layers.** Make the
   local backend produce the same `DriveObject` / `Agent` /
   `ConversationMessage` types the cloud backend produces. Callers
   only see one type, and the swap is invisible.
2. **Settings-driven, not environment-driven.** Configuration lives
   in `~/.wish/settings.toml` so users can manage it like every
   other Wish setting (and our settings-export bundle includes it
   for support).
3. **Empty state is a first-class state.** Both panels need a
   recognizable empty state with concrete next steps ("Create
   workflow", "Configure Ollama"). No "{}" or blank surfaces.
4. **Local IDs are namespaced.** `local:<bare>` for Drive,
   `builtin:<slug>` for built-in agents. Future hybrid mode (some
   local + some Hermon) shouldn't need to migrate IDs.
5. **Privacy by default.** API keys, file paths, and any
   identifiable user data is masked when serializing for telemetry
   or support bundles.

## Cross-references

- `docs/LOCAL_DEV.md` — user-facing setup guide (Ollama,
  local Hermon backend, login flow)
- `docs/AGENT_REGISTRY.md` — the registry that powers the agent
  catalog (already tested + wired)
- `docs/HERMON_ECOSYSTEM.md` — high-level architecture diagram
- `docs/UPSTREAM_SYNC.md` — cherry-pick policy + history
