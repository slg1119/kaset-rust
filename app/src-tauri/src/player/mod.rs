//! Playback. `queue` and `engine` are pure Rust; the other modules are the
//! Tauri glue around the hidden player window.

mod autoplay;
pub mod commands;
pub mod engine;
pub mod media;
pub mod queue;
pub mod service;
pub mod video_id;
pub mod window;
