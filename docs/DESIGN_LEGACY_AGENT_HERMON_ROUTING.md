# Design: legacy agent runtime → Hermon routing

**Status**: design, implementation not started.

## What "legacy agent" means

Wish has two chat surfaces today:

1. **`LocalLlmAdapter`** (used by `wish_conversation`) — already routed by model-id prefix (commit `94b6fed2`). Picks `hermon-local:`, `hermon:`, `ollama:` and dispatches to the right endpoint. End-to-end working.
2. **Legacy Warp agent runtime** (used by the original Warp-style agent CLI surfaces) — talks GraphQL to the historical Warp server. Doesn't speak the Hermon `/v1/chat/completions` shape.

The legacy runtime is what powers `wish agent`, the terminal AI command suggestions, and the Cmd+I "explain this command" flow. Today these all 500 when run against `api.hermon.ai` because the underlying GraphQL queries (`AmQuerySuggestions`, `InputSuggestions`, etc.) aren't implemented server-side. We stubbed the worst two endpoints to return empty results in v0.6.0; the rest still fail.

## Two viable paths

### Path A — Translate GraphQL → OpenAI on the gateway

Add a `/graphql` route in `hermon-gateway` that parses Warp's incoming GraphQL request, extracts the user prompt + context, and dispatches via the existing `LlmRouter`. Translate the OpenAI streaming response back into Warp's GraphQL subscription shape.

**Pros**: zero client changes. Every legacy surface "just works."

**Cons**: enormous matching surface — Warp has ~40 distinct GraphQL operations. Translating each one is a non-trivial reverse-engineering exercise from the wire. Easy to get subtly wrong (escape semantics, tool-call shapes).

### Path B — Replace the runtime call site

Swap the GraphQL client in `app/src/ai/blocklist/controller.rs` (and friends) with a thin OpenAI client that hits `${hermon_root}/v1/chat/completions`. Re-stream into the existing `BlocklistAIController` event types.

**Pros**: surgical. Touches one client crate. The gateway side is already done.

**Cons**: ~20 call sites. Each one constructs a different GraphQL operation with different field selections. Need to map each prompt-shape into a chat message + system prompt pair.

## Recommendation

**Path B**, done incrementally. Start with `AmQuerySuggestions` (the "what command should I run next" feature) since it's the highest-traffic. The model is fed a system prompt + the user's recent terminal history; the response is a list of suggested commands. One PR, ~150 lines, covers the most-visible surface without needing to design a GraphQL compatibility layer.

After that, work outward: `InputSuggestions`, `ExplainCommand`, `BlocklistFollowup`. Each is its own PR with its own prompt design.

## What blocks shipping Path B today

1. The `BlocklistAIController` event types are baked into the GraphQL subscription shape. Need to confirm we can synthesize them from OpenAI chunks without breaking downstream consumers.
2. The system prompts for each Warp operation aren't extracted anywhere — they're implicit in the server's behavior. Need a one-time reverse-engineering pass: capture the prompt one of Anthropic's models would need to produce equivalent output.
3. Telemetry — each Warp op emits its own telemetry event. Need to map those to whatever generic LLM-call telemetry we want for Hermon.

None of these are blockers, just work items.

## Implementation effort

- Single-op MVP (just `AmQuerySuggestions`): ~150 lines, 2-3 hours.
- All four high-traffic ops: ~600 lines, 1-2 days.
- Full legacy coverage: 1-2 weeks.

The single-op MVP is the right "shipped, end-to-end" deliverable for a focused round.
