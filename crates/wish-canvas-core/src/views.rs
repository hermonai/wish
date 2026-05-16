//! View projections: derive a [`Canvas`] from a [`WishWorld`].
//!
//! Implementations of the v0.5.0 views live in the crates that own the
//! source data:
//! - `RepoMap` is materialized in `wish-codegraph`.
//! - `AgentDag` in `wish-agent-visualizer`.
//! - `CommandTimeline` and `GitWorldline` in `wish-canvas` (uses local
//!   terminal history + git metadata).
//!
//! This module just declares the projection function signature so all
//! callers can agree.

use crate::types::{Canvas, CanvasView};
use wish_world_model::WishWorld;

/// A projection produces a [`Canvas`] from a [`WishWorld`] for a given
/// [`CanvasView`].
pub trait Projection {
    fn view(&self) -> CanvasView;
    fn project(&self, world: &WishWorld) -> Canvas;
}
