//! Attack (onset) detection with single-sample resolution.
//!
//! Two detectors run in parallel, each comparing a 1 ms mean-|x| envelope with a background
//! that follows the *peaks* of that envelope (3 ms rise, slow fall). Within a sustained note the
//! envelope swings with the waveform (a low E has 12 ms periods) but never exceeds its own
//! recent peaks, so only a genuine new attack can jump above the background:
//! * full band — notes rising out of silence or out of a decayed note;
//! * high band (> 2.5 kHz) — pick noise of a re-attack on a still-ringing string, whose
//!   high-frequency content has already died out.

use crate::db_to_lin;
use crate::fx::biquad::Biquad;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OnsetConfig {
    pub sample_rate: f32,
    /// Level (dBFS of the 1 ms mean-|x| envelope) an attack must exceed.
    pub min_level_db: f32,
    /// Full-band jump over the background envelope that counts as an attack.
    pub ratio_db: f32,
    /// High-band jump over its background that counts as a re-attack.
    pub hf_ratio_db: f32,
    /// Minimum high-band envelope for a re-attack (dBFS).
    pub hf_min_level_db: f32,
    /// Dead time after an onset.
    pub refractory_ms: f32,
}

impl Default for OnsetConfig {
    fn default() -> Self {
        Self {
            sample_rate: crate::SAMPLE_RATE,
            min_level_db: -60.0,
            ratio_db: 6.0,
            hf_ratio_db: 12.0,
            hf_min_level_db: -66.0,
            refractory_ms: 35.0,
        }
    }
}

pub struct OnsetDetector {
    fast: f32,
    slow: f32,
    hf_fast: f32,
    hf_slow: f32,
    hp: Biquad,
    c_fast: f32,
    c_up: f32,
    c_down: f32,
    c_hf_down: f32,
    min_level: f32,
    ratio: f32,
    hf_ratio: f32,
    hf_min: f32,
    refractory: u32,
    dead: u32,
}

fn coef(ms: f32, sr: f32) -> f32 {
    1.0 - (-1.0 / (ms * 1e-3 * sr)).exp()
}

impl OnsetDetector {
    pub fn new(cfg: OnsetConfig) -> Self {
        let sr = cfg.sample_rate;
        Self {
            fast: 0.0,
            slow: 0.0,
            hf_fast: 0.0,
            hf_slow: 0.0,
            hp: Biquad::highpass(sr, 2500.0, 0.707),
            c_fast: coef(1.0, sr),
            c_up: coef(3.0, sr),
            c_down: coef(150.0, sr),
            c_hf_down: coef(80.0, sr),
            min_level: db_to_lin(cfg.min_level_db),
            ratio: db_to_lin(cfg.ratio_db),
            hf_ratio: db_to_lin(cfg.hf_ratio_db),
            hf_min: db_to_lin(cfg.hf_min_level_db),
            refractory: (cfg.refractory_ms * 1e-3 * sr) as u32,
            dead: 0,
        }
    }

    /// Feeds `x`, whose first sample has absolute index `start`; returns the absolute index of
    /// the first onset found in it.
    pub fn process(&mut self, x: &[f32], start: u64) -> Option<u64> {
        let mut found = None;
        for (i, &s) in x.iter().enumerate() {
            self.fast += self.c_fast * (s.abs() - self.fast);
            let c = if self.fast > self.slow { self.c_up } else { self.c_down };
            self.slow += c * (self.fast - self.slow);

            let h = self.hp.process(s).abs();
            self.hf_fast += self.c_fast * (h - self.hf_fast);
            let c = if self.hf_fast > self.hf_slow { self.c_up } else { self.c_hf_down };
            self.hf_slow += c * (self.hf_fast - self.hf_slow);

            if self.dead > 0 {
                self.dead -= 1;
                continue;
            }
            let full = self.fast > self.min_level && self.fast > self.ratio * self.slow;
            let high = self.hf_fast > self.hf_min && self.hf_fast > self.hf_ratio * self.hf_slow;
            if (full || high) && found.is_none() {
                found = Some(start + i as u64);
                self.dead = self.refractory;
            }
        }
        found
    }

    /// Current 1 ms mean-|x| envelope (linear).
    pub fn level(&self) -> f32 {
        self.fast
    }
}
