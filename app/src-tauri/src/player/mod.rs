//! Playback. `queue` and `engine` are pure Rust; the other modules are the
//! Tauri glue around the hidden player window.

pub mod engine;
pub mod queue;
pub mod video_id;
