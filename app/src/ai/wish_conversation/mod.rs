//! Wish conversation surface — chat with SDLC agents.
//!
//! This is the data-model layer that backs the chat view (the
//! killer feature). The view is built in a follow-up turn; this
//! turn ships the model + adapters so the contract is locked.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │  ConversationManagerModel  (singleton)                   │
//! │    • all conversations + per-convo in-flight stream      │
//! │    • events: Created · TurnAppended · Chunk ·            │
//! │              ActiveChanged · Removed                     │
//! └─────────────────────┬────────────────────────────────────┘
//!                       │ subscribes
//!                       ▼
//!     ┌────────────────────────────────┐
//!     │  WishChatView (next turn)      │
//!     │    renders projection          │
//!     └────────────────────────────────┘
//!
//!     ConversationAdapter trait:
//!       fn send(conv, msg, sink: ChunkSink) emits StreamChunk
//!     Implementations:
//!       StubAdapter        ✅ shipped (tests + demo button)
//!       LocalLlmAdapter    ✅ shipped (Ollama / OpenAI-compat)
//!       HermonAdapter      ⏳ next turn (full features when logged in)
//! ```
//!
//! # Quick start
//!
//! ```no_run
//! # use wish::ai::wish_conversation::{
//! #     ConversationManagerModel, StubAdapter, ConversationAdapter, StreamChunk,
//! # };
//! # use wishui::{AppContext, SingletonEntity};
//! # fn example(ctx: &mut AppContext) {
//! let mgr = ConversationManagerModel::handle(ctx);
//! let convo_id = mgr.update(ctx, |m, ctx| m.create("wish-coder".into(), ctx));
//! mgr.update(ctx, |m, ctx| {
//!     m.append_user(&convo_id, "Help me refactor this".into(), ctx);
//! });
//!
//! // Drive a stub adapter:
//! let adapter = StubAdapter::new();
//! let convo_snapshot = mgr.as_ref(ctx).get(&convo_id).unwrap().clone();
//! let convo_id_for_sink = convo_id.clone();
//! adapter.send(
//!     &convo_snapshot,
//!     "Help me refactor this",
//!     Box::new(move |chunk| {
//!         // In real code, dispatch through a workspace action so
//!         // the chunk reaches the singleton on the right thread.
//!         drop((chunk, &convo_id_for_sink));
//!     }),
//! );
//! # }
//! ```

pub mod adapter;
pub mod local_llm_adapter;
pub mod model;
pub mod types;
pub mod view;

#[cfg(test)]
mod tests;

pub use adapter::{ChunkSink, ConversationAdapter, StubAdapter};
pub use local_llm_adapter::LocalLlmAdapter;
pub use model::{ConversationManagerEvent, ConversationManagerModel, InFlight};
pub use types::{derive_title_from, Conversation, ConversationId, MessageBlock, StreamChunk, Turn};
pub use view::WishChatView;
