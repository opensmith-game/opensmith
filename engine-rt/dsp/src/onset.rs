//! Attack (onset) detection with single-sample resolution.
//!
//! Two detectors run in parallel, each comparing a 1 ms mean-|x| envelope with a background
//! that follows the *peaks* of that envelope (3 ms rise, slow fall). Within a sustained note the
//! envelope swings with the waveform (a low E has 12 ms periods) but never exceeds its own
//! recent peaks, so only a genuine new attack can jump above the background:
//! * full band — notes rising out of silence or out of a decayed note;
//! * high band (> 2.5 kHz) — pick noise of a re-attack on a still-ringing string, whose
//!   high-frequency content has already died out;
//! * high-band rise — the same band against its own highest level 8 to 25 ms earlier: a chord
//!   strummed again while the last one still rings, even softer than it, jumps out of what was
//!   sounding just before, while a ringing chord never does.

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
    /// High-band rise over its highest level 8 to 25 ms earlier that counts as a re-attack.
    pub rise_db: f32,
    /// Dead time after an onset.
    pub refractory_ms: f32,
}

impl Default for OnsetConfig {
    fn default() -> Self {
        Self {
            sample_rate: crate::SAMPLE_RATE,
            min_level_db: -60.0,
            ratio_db: 6.0,
            hf_ratio_db: 9.0,
            hf_min_level_db: -66.0,
            rise_db: 6.0,
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
    rise: f32,
    /// High-band envelope maxima per millisecond, the last `PAST_MS` of them.
    past: [f32; PAST_MS],
    past_at: usize,
    ms_len: u32,
    in_ms: u32,
    ms_max: f32,
    /// Highest high-band level 8 to 25 ms ago.
    before: f32,
    /// Where the high band went above `before` (the start of a rise), if it still is.
    rising_since: Option<u64>,
}

/// Milliseconds of high-band history kept for the rise detector.
const PAST_MS: usize = 25;
/// The most recent milliseconds left out of the rise detector's reference (the attack itself).
const RECENT_MS: usize = 8;
/// A rise must also clear the high band's background by this much (linear, 4 dB).
const RISE_FLOOR: f32 = 1.585;

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
            rise: db_to_lin(cfg.rise_db),
            past: [0.0; PAST_MS],
            past_at: 0,
            ms_len: (1e-3 * sr).round().max(1.0) as u32,
            in_ms: 0,
            ms_max: 0.0,
            before: f32::MAX,
            rising_since: None,
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
            self.ms_max = self.ms_max.max(self.hf_fast);
            self.in_ms += 1;
            if self.in_ms == self.ms_len {
                self.past[self.past_at] = self.ms_max;
                self.past_at = (self.past_at + 1) % PAST_MS;
                self.in_ms = 0;
                self.ms_max = 0.0;
                // Oldest first from `past_at`: all but the most recent milliseconds.
                self.before = (0..PAST_MS - RECENT_MS).map(|k| self.past[(self.past_at + k) % PAST_MS]).fold(0.0, f32::max);
            }

            if self.hf_fast > self.before {
                self.rising_since.get_or_insert(start + i as u64);
            } else {
                self.rising_since = None;
            }
            if self.dead > 0 {
                self.dead -= 1;
                continue;
            }
            let full = self.fast > self.min_level && self.fast > self.ratio * self.slow;
            let high = self.hf_fast > self.hf_min && self.hf_fast > self.hf_ratio * self.hf_slow;
            // Over what sounded just before, and over the band's own background too (beating
            // partials of a ringing note swing above their recent past, not that far).
            let rise = self.hf_fast > self.hf_min && self.hf_fast > self.rise * self.before && self.hf_fast > RISE_FLOOR * self.hf_slow;
            if (full || high || rise) && found.is_none() {
                // A rise is dated from where it began, a few ms back at most.
                let at = start + i as u64;
                let began = self.rising_since.filter(|&b| at - b <= self.ms_len as u64 * 4);
                found = Some(if rise && !full && !high { began.unwrap_or(at) } else { at });
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
