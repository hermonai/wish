#[cfg(winit)]
pub mod winit;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub use winit::WindowingSystem;
pub use wishui_core::windowing::*;
