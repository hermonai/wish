//! Wish World Studio — deterministic world-builder agents.
//!
//! v0.5.0 ships a single deterministic builder: `build_shanhai_harbor`.
//! It emits a sequence of `WorldPatch`es that, when applied through
//! `wish-provenance::apply_with_provenance`, construct the Shan Hai
//! Fintech Harbor demo world end-to-end — code, scenes, NPCs, the
//! World Architect agent — *with* a full WorldLine.
//!
//! This is the offline-runnable substrate the North Star demo sits on.
//! Later, this same shape will receive patches from a live Hermon
//! agent over the agent visualizer channel.

pub mod builders;
pub mod canvas_views;
pub mod intent;
pub mod viewer;

pub use builders::{build_shanhai_harbor, ShanHaiBuild};
pub use canvas_views::world_to_canvas;
pub use intent::{apply_plan, plan_world, WorldPlan};
pub use viewer::{canvas_html, world_html};
