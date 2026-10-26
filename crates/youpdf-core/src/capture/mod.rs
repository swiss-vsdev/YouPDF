//! Capture subsystem: screenshots, clipboard, TTS.
//!
//! Wave 1 Agent A5 (TTS) implements `tts.rs`.
//! Wave 3 Agent C4 (screenshots) implements `screenshot.rs`.
//!
//! ponytail: Windows/Linux stubs return `Err("Platform not supported")`; add
//! `speech-dispatcher` / SAPI / `flameshot` later if cross-platform is needed.

pub mod screenshot;
pub mod tts;
pub use screenshot::{capture_region_interactive, capture_window, clipboard_as_png};
pub use tts::TtsEngine;