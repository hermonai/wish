//! Wish Canvas — the WishUI pane integration for the 2D semantic canvas.
//!
//! v0.5.0 scaffold (step 01 of the implementation plan): defines the
//! public API surface (commands, controller events, agent sink). The
//! WishUI [`Element`] registration and the `wishui_extras` wiring land
//! in `v0.5.0-step-05`.

pub mod agent_sink;
pub mod commands;
pub mod controller;
pub mod views;

pub use commands::CanvasCommand;
pub use controller::CanvasInput;
