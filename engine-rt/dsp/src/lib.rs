//! OpenSmith real-time DSP core (Engine-RT, module A).
//!
//! Everything on a processing path (`process`, `detect`, `verify`) is deterministic and
//! allocation-free: buffers are sized at construction, so these objects can live inside a
//! CoreAudio callback. Allocation-freedom is enforced by `tests/rt_safety.rs`.

pub mod chord;
pub mod fft;
pub mod fretboard;
pub mod fx;
pub mod guitar;
pub mod note;
pub mod notefeat;
pub mod notenet;
pub mod onset;
pub mod pitch;
pub mod synth;
pub mod tracker;
pub mod tuner;
pub mod wav;

pub use chord::{ChordConfig, ChordMatch, ChordVerifier};
pub use fretboard::{FretPos, Tuning};
pub use fx::amp::{AmpKnobs, AmpModel};
pub use fx::chain::{ChainParams, GuitarChain};
pub use pitch::{Mpm, PitchConfig, PitchDetector, PitchEstimate, Yin};
pub use tracker::{Algorithm, NoteTracker, TrackerConfig, TrackerEvent};
pub use tuner::{TunerAnalyzer, TunerConfig, TunerReading};

/// Engine sample rate. Every component accepts others, but 48 kHz is the contract with CoreAudio.
pub const SAMPLE_RATE: f32 = 48_000.0;

/// Converts decibels (full scale) to linear amplitude.
#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Converts linear amplitude to decibels (full scale), floored at -240 dB.
#[inline]
pub fn lin_to_db(lin: f32) -> f32 {
    20.0 * lin.abs().max(1e-12).log10()
}
