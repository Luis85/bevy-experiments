//! Audio decisions and tables for Last Light, without Bevy.
//!
//! This crate owns every audio decision that can be tested without an audio
//! device: cue and bank tables, the mix table, selection, the mixer state
//! machine, DSP shared by the runtime and the offline renderer, the loudness
//! meter, and synthesis. `horde_game` keeps only the Bevy adapter that
//! executes what this crate decides.
//!
//! Rules that hold for every module here:
//!
//! - No Bevy and no external crate beyond `serde` and `serde_json`; the test
//!   `horde_audio_builds_without_bevy` checks the dependency tree.
//! - Audio is presentation: nothing here reads the core RNG or changes game
//!   state. Variation comes from presentation counters.
//! - SHA-256 for asset audits is `horde_core::sha256`, reached through the
//!   test-support dev-dependency only.

pub mod bank;
pub mod cue;
pub mod events;
pub mod export;
pub mod meter;
pub mod mix;
pub mod pan;
pub mod render;
pub mod select;
pub mod synth;
pub mod voices;
pub mod wav;
