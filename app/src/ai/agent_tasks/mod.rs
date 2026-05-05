//! Client-side SDLC agent task tracking — the data model behind the
//! "Tasks" panel and the conversation-inline annotation surface.
//!
//! See [`model`] for the architecture overview, [`types`] for the
//! pure data types.
//!
//! # Quick start
//!
//! ```no_run
//! # use wish::ai::agent_tasks::{
//! #     AgentTaskRegistryModel, ToolKind, TaskStatus, TaskAnnotation,
//! # };
//! # use wishui::{AppContext, SingletonEntity};
//! # fn example(ctx: &mut AppContext) {
//! // From a view, get a handle to the registry:
//! let registry = AgentTaskRegistryModel::handle(ctx);
//!
//! // To create a task (typically called from the agent runtime
//! // when it invokes a tool):
//! let id = registry.update(ctx, |r, ctx| {
//!     r.create("Run all my tests", ToolKind::Bash, false, ctx)
//! });
//!
//! // Advance through the lifecycle:
//! registry.update(ctx, |r, ctx| {
//!     r.set_status(&id, TaskStatus::Running, ctx);
//!     r.add_annotation(
//!         &id,
//!         TaskAnnotation::CommandRun {
//!             description: "cargo test".into(),
//!             exit_code: None,
//!         },
//!         ctx,
//!     );
//! });
//! # }
//! ```

pub mod model;
pub mod types;

#[cfg(test)]
mod tests;

pub use model::{AgentTaskEvent, AgentTaskRegistryModel};
pub use types::{AgentTask, TaskAnnotation, TaskId, TaskStatus, ToolKind};
