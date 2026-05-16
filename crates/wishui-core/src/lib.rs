#[macro_use]
extern crate num_derive;

pub mod accessibility;
pub mod actions;
mod app_focus_telemetry;
pub mod assets;
pub mod r#async;
pub mod clipboard;
pub mod clipboard_utils;
mod core;
mod debug;
pub mod elements;
pub mod event;
pub mod fonts;
/// **Generative UI substrate** — JSON descriptors → Scene.
///
/// See `wish-design/.../01-strategy/10-wishui-generative-ui.md` for
/// the strategic roadmap. This module is the wire format AI agents
/// use to emit UI without knowing about the GPU pipeline.
pub mod generative;
pub mod image_cache;
pub mod integration;
pub mod keymap;
pub mod modals;
pub mod notification;
pub mod platform;
pub mod prelude;
pub mod presenter;
pub mod rendering;
pub mod scene;
pub mod telemetry;
#[cfg(test)]
mod test;
pub mod text;
pub mod text_layout;
pub mod text_selection_utils;
pub mod time;
pub mod traces;
pub mod ui_components;
pub mod units;
pub mod util;
pub mod windowing;
pub mod zoom;

pub use crate::core::*;
pub use assets::AssetProvider;
pub use clipboard::Clipboard;
pub use elements::Element;
pub use event::Event;
pub use pathfinder_color as color;
pub use pathfinder_geometry as geometry;
pub use presenter::{
    AfterLayoutContext, EventContext, LayoutContext, PaintContext, Presenter, SizeConstraint,
};
pub use scene::{ClipBounds, Scene};
pub use zoom::ZoomFactor;

use pathfinder_color::ColorU;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gradient {
    pub start: ColorU,
    pub end: ColorU,
}
