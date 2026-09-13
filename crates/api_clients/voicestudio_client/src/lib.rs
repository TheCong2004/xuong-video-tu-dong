//! Small, transport-only client for VoiceStudio's local speech platform.
//!
//! This crate intentionally does not embed VoiceStudio or its Python runtime.
//! ArtCraft talks to the independently managed service through the stable
//! discovery and OpenAI-compatible audio endpoints.

mod client;
mod error;
mod provider;
mod types;

pub use client::VoiceStudioClient;
pub use error::VoiceStudioClientError;
pub use provider::VoiceProvider;
pub use types::{DubRequest, DubSegment, SpeechRequest, SpeechResponse, TranscriptionRequest, TranscriptionResponse, TranslateRequest, TranslateSegment};
