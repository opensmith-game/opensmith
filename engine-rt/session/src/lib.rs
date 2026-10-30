//! A game session: the audio engine plays the backing track and listens to the guitar, the game
//! judges what it hears against the chart, and the UI receives a stream of events.
//!
//! Timing: the output callback publishes when each backing-track frame reaches the listener, the
//! input callback when each guitar sample was captured, both in host time. An attack is placed in
//! the song where the player *heard* it, whatever the latencies of the two devices.

mod calibration;
mod events;
mod history;
mod runner;
mod session;
mod track;

pub use calibration::{Calibration, CalibrationPlan, CalibrationResult, MIN_HITS};
pub use events::{UiEvent, UiExpected, UiScore, UiTarget};
pub use history::TapHistory;
pub use runner::{Command, Devices, EngineBackend, Poll, SessionHandle};
pub use session::{AudioBackend, RiffLoop, Session, SessionConfig};
pub use track::{load_track, TrackError};
