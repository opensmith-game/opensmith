//! Shared helpers for the Step 1 DSP test battery.
#![allow(dead_code)]

use opensmith_dsp::note;
use opensmith_dsp::synth::{karplus_strong, sine, Pcg32, Pluck};
use opensmith_dsp::{NoteTracker, TrackerEvent};

pub const SR: f32 = 48_000.0;
/// CoreAudio buffer size targeted by the engine.
pub const BLOCK: usize = 64;

/// Open strings with the frequencies quoted in the spec, low E first: (name, Hz, MIDI).
pub const OPEN_STRINGS: [(&str, f32, i32); 6] = [
    ("E2", 82.41, 40),
    ("A2", 110.0, 45),
    ("D3", 146.83, 50),
    ("G3", 196.0, 55),
    ("B3", 246.94, 59),
    ("E4", 329.63, 64),
];

pub fn midi_freq(midi: i32) -> f32 {
    note::midi_to_freq(midi as f32, note::A4_HZ)
}

pub fn ms(samples: u64) -> f32 {
    samples as f32 * 1000.0 / SR
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Pure sinusoid.
    Sine,
    /// Additive plucked string, random pluck/pickup position, tilt, stiffness, pick noise.
    Pluck,
    /// Karplus–Strong string.
    Ks,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Sine, Kind::Pluck, Kind::Ks];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Sine => "sine",
            Kind::Pluck => "pluck",
            Kind::Ks => "karplus",
        }
    }
}

/// One deterministic note; `seed` picks the timbre variant.
pub fn render(kind: Kind, freq: f32, secs: f32, amp: f32, seed: u64) -> Vec<f32> {
    let mut rng = Pcg32::new(seed);
    match kind {
        Kind::Sine => sine(freq, SR, secs, amp),
        Kind::Pluck => {
            let mut p = Pluck::new(freq);
            p.secs = secs;
            p.amp = amp;
            p.pluck_pos = rng.range(0.08, 0.45);
            p.pickup_pos = if rng.uniform() < 0.5 { rng.range(0.06, 0.25) } else { 0.0 };
            p.tilt = rng.range(1.0, 2.0);
            p.inharmonicity = rng.range(2e-5, 2e-4);
            p.pick_noise = rng.range(0.0, 0.6);
            p.t60 = rng.range(1.5, 4.0);
            p.render(SR, &mut rng)
        }
        Kind::Ks => {
            let (pos, t60, noise) = (rng.range(0.1, 0.45), rng.range(1.5, 4.0), rng.range(0.0, 0.3));
            karplus_strong(freq, SR, secs, amp, pos, t60, noise, &mut rng)
        }
    }
}

/// Feeds `signal` to the tracker in CoreAudio-sized blocks and collects every event.
pub fn run_tracker(tracker: &mut NoteTracker, signal: &[f32]) -> Vec<TrackerEvent> {
    let mut events = Vec::new();
    for chunk in signal.chunks(BLOCK) {
        tracker.process(chunk, |e| events.push(e));
    }
    events
}

#[derive(Clone, Copy, Debug)]
pub struct On {
    pub onset: u64,
    pub sample: u64,
    pub midi: i32,
    pub cents: f32,
    pub legato: bool,
}

pub fn note_ons(events: &[TrackerEvent]) -> Vec<On> {
    events
        .iter()
        .filter_map(|e| match *e {
            TrackerEvent::NoteOn { onset_sample, sample, midi, cents, legato, .. } => {
                Some(On { onset: onset_sample, sample, midi, cents, legato })
            }
            _ => None,
        })
        .collect()
}
