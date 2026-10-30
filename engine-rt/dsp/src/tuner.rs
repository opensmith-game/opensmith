//! Tuner analysis: the fundamental of a ringing string to a fraction of a cent.
//!
//! The note tracker measures the waveform's period on a short window, for the game's latency.
//! That is not precise enough to tune: the upper partials of a real string are stretched
//! (inharmonicity) and beat against each other, so the period of the raw waveform drifts and
//! jumps by several cents, up to twenty and more on a low string. Here, on a long window
//! (170 ms): a coarse period by MPM, then the window low-passed just above its first partials
//! (zero-phase, so nothing is delayed), and the period measured again across as many periods as
//! the window holds, which divides the error of the peak's position by that many.

use std::f64::consts::TAU;

use crate::pitch::{Mpm, PitchConfig, PitchDetector};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerConfig {
    pub sample_rate: f32,
    /// Analysis window (samples): eleven periods of a low C at 48 kHz.
    pub window: usize,
    pub min_freq: f32,
    pub max_freq: f32,
    /// Low-pass cutoff, in multiples of the coarse fundamental.
    pub cutoff: f64,
    /// Below this periodicity (normalised correlation) the string is not ringing clearly.
    pub min_clarity: f32,
    /// Below this RMS level (dBFS) nothing is reported.
    pub min_level_db: f32,
}

impl Default for TunerConfig {
    fn default() -> Self {
        Self {
            sample_rate: crate::SAMPLE_RATE,
            window: 8192,
            min_freq: 55.0,
            max_freq: 1400.0,
            cutoff: 3.5,
            min_clarity: 0.8,
            min_level_db: -72.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerReading {
    pub freq_hz: f64,
    /// Fractional MIDI note (A4 = 69 = 440 Hz).
    pub midi: f64,
    pub clarity: f32,
    pub level_db: f32,
}

pub struct TunerAnalyzer {
    cfg: TunerConfig,
    mpm: Mpm,
    low: Vec<f64>,
    low32: Vec<f32>,
}

/// Edge of the low-passed window left out of the fine measurement (the filter's start-up).
const EDGE: usize = 1024;

impl TunerAnalyzer {
    pub fn new(cfg: TunerConfig) -> Self {
        let pitch = PitchConfig {
            sample_rate: cfg.sample_rate,
            min_freq: cfg.min_freq,
            max_freq: cfg.max_freq,
            max_window: cfg.window,
            min_overlap: 1.0,
            ..PitchConfig::default()
        };
        Self { cfg, mpm: Mpm::new(pitch), low: vec![0.0; cfg.window], low32: vec![0.0; cfg.window] }
    }

    pub fn window(&self) -> usize {
        self.cfg.window
    }

    /// The pitch of the last `window` samples of `x`, if a string rings clearly.
    pub fn analyze(&mut self, x: &[f32]) -> Option<TunerReading> {
        let n = self.cfg.window;
        if x.len() < n {
            return None;
        }
        let x = &x[x.len() - n..];
        let energy = x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / n as f64;
        let level_db = (10.0 * energy.max(1e-20).log10()) as f32;
        if level_db < self.cfg.min_level_db {
            return None;
        }
        let coarse = self.mpm.detect(x)?;
        let sr = self.cfg.sample_rate as f64;
        lowpass(x, self.cfg.cutoff * coarse.freq_hz as f64, sr, &mut self.low);
        self.low32.iter_mut().zip(&self.low).for_each(|(d, &s)| *d = s as f32);
        // The coarse period again on the low-passed window: the first partials alone.
        let period = self.mpm.detect(&self.low32).map_or(coarse.period, |e| e.period) as f64;
        let (period, clarity) = refine(&self.low[EDGE..n - EDGE], period)?;
        if (clarity as f32) < self.cfg.min_clarity {
            return None;
        }
        let freq_hz = sr / period;
        if freq_hz < self.cfg.min_freq as f64 || freq_hz > self.cfg.max_freq as f64 {
            return None;
        }
        Some(TunerReading { freq_hz, midi: 69.0 + 12.0 * (freq_hz / 440.0).log2(), clarity: clarity as f32, level_db })
    }
}

/// Zero-phase 4th-order Butterworth low-pass (two biquads, forward then backward).
fn lowpass(x: &[f32], cutoff: f64, sr: f64, out: &mut [f64]) {
    out.iter_mut().zip(x).for_each(|(o, &v)| *o = v as f64);
    let w = TAU * cutoff.min(0.45 * sr) / sr;
    let (sin, cos) = (w.sin(), w.cos());
    for q in [0.541_196_1, 1.306_563] {
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1) = ((1.0 - cos) / 2.0 / a0, (1.0 - cos) / a0);
        let (a1, a2) = (-2.0 * cos / a0, (1.0 - alpha) / a0);
        let biquad = |y: &mut dyn Iterator<Item = &mut f64>| {
            let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
            for v in y {
                let o = b0 * *v + b1 * x1 + b0 * x2 - a1 * y1 - a2 * y2;
                (x2, x1, y2, y1) = (x1, *v, y1, o);
                *v = o;
            }
        };
        biquad(&mut out.iter_mut());
        biquad(&mut out.iter_mut().rev());
    }
}

/// Normalised correlation of `x` with itself `lag` samples later.
fn correlation(x: &[f64], lag: usize) -> f64 {
    let (mut r, mut a, mut b) = (0.0, 0.0, 0.0);
    for j in 0..x.len() - lag {
        let (u, v) = (x[j], x[j + lag]);
        r += u * v;
        a += u * u;
        b += v * v;
    }
    r / (a * b).sqrt().max(1e-20)
}

/// The period measured across `k` periods (as many as fit in half the window): the correlation
/// peak near `k · period`, interpolated, divided by `k`. Also returns the peak's height.
fn refine(x: &[f64], period: f64) -> Option<(f64, f64)> {
    let k = ((x.len() as f64 / 2.0) / period).floor().max(1.0);
    let centre = (k * period).round() as usize;
    let (lo, hi) = (centre.checked_sub(4)?, centre + 4);
    if hi + 1 >= x.len() {
        return None;
    }
    let peak = (lo..=hi).map(|lag| (lag, correlation(x, lag))).max_by(|a, b| a.1.total_cmp(&b.1))?;
    let (y0, y1, y2) = (correlation(x, peak.0 - 1), peak.1, correlation(x, peak.0 + 1));
    let denom = y0 - 2.0 * y1 + y2;
    let delta = if denom.abs() < 1e-15 { 0.0 } else { (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5) };
    Some(((peak.0 as f64 + delta) / k, y1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{white_noise, Pcg32, Pluck};

    const SR: f32 = 48_000.0;

    /// A string plucked a little sharp, through a bridge-side pickup, with some hum-free noise:
    /// the readings over its ringing, in cents from its true fundamental.
    fn errors(freq: f32, inharmonicity: f32, seed: u64) -> Vec<f64> {
        let detuned = freq * 2f32.powf(7.0 / 1200.0);
        let mut rng = Pcg32::new(seed);
        let pluck = Pluck { secs: 3.5, pickup_pos: 0.12, inharmonicity, t60: 4.0, amp: 0.4, ..Pluck::new(detuned) };
        let mut sig = pluck.render(SR, &mut rng);
        let noise = white_noise(&mut rng, sig.len(), 0.0005);
        for (s, n) in sig.iter_mut().zip(noise) {
            *s += n;
        }
        let truth = detuned as f64 * (1.0 + inharmonicity as f64).sqrt();
        let mut tuner = TunerAnalyzer::new(TunerConfig::default());
        let mut out = Vec::new();
        let mut end = (0.3 * SR) as usize;
        while end <= (3.0 * SR) as usize {
            let r = tuner.analyze(&sig[..end]).expect("a ringing string is read");
            out.push(1200.0 * (r.freq_hz / truth).log2());
            end += 960;
        }
        out
    }

    #[test]
    fn reads_every_string_to_a_fraction_of_a_cent() {
        // Drop C's low C up to the high e, the low strings stiffer.
        for (i, &(f, b)) in [(65.406, 1.5e-4), (82.407, 1.5e-4), (110.0, 8e-5), (146.832, 8e-5), (195.998, 8e-5), (246.942, 8e-5), (329.628, 8e-5)]
            .iter()
            .enumerate()
        {
            let e = errors(f, b, 11 + i as u64);
            let mean = e.iter().sum::<f64>() / e.len() as f64;
            let sd = (e.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / e.len() as f64).sqrt();
            let worst = e.iter().fold(0f64, |m, x| m.max(x.abs()));
            assert!(mean.abs() < 1.0 && sd < 0.6 && worst < 3.0, "{f} Hz: mean {mean:+.2}, sd {sd:.2}, worst {worst:.2} cents");
        }
    }

    #[test]
    fn says_nothing_on_silence_or_noise() {
        let mut tuner = TunerAnalyzer::new(TunerConfig::default());
        assert!(tuner.analyze(&vec![0.0; 8192]).is_none());
        let noise = white_noise(&mut Pcg32::new(3), 8192, 0.05);
        assert!(tuner.analyze(&noise).is_none());
    }
}
