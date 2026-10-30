//! Monophonic pitch detection: McLeod Pitch Method (primary) and YIN (cross-check).
//!
//! Both run on an arbitrary-length window (the tracker grows it from the note onset) and both
//! refuse to report a period they cannot see repeat: MPM only searches lags whose overlap with
//! the window is at least `min_overlap` × lag, YIN only lags ≤ half the window.

pub mod mpm;
pub mod yin;

pub use mpm::Mpm;
pub use yin::Yin;

/// One pitch measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchEstimate {
    pub freq_hz: f32,
    /// Fundamental period in samples (fractional).
    pub period: f32,
    /// Periodicity confidence in 0..=1 (MPM: NSDF peak height, YIN: 1 − CMNDF dip depth).
    pub clarity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchConfig {
    pub sample_rate: f32,
    /// Lowest detectable fundamental (60 Hz covers drop-C and B-standard low strings).
    pub min_freq: f32,
    /// Highest detectable fundamental (24th fret of the high e is 1318.5 Hz).
    pub max_freq: f32,
    /// Longest analysis window, in samples.
    pub max_window: usize,
    /// MPM: minimum overlap at a candidate lag, as a fraction of that lag. Smaller reports
    /// earlier after an onset; the clarity threshold of the tracker keeps it honest.
    pub min_overlap: f32,
    /// Minimum overlap in samples whatever the lag (2 ms): a correlation measured over fewer
    /// samples is statistically meaningless for high notes.
    pub min_overlap_samples: usize,
    /// MPM: McLeod's `k`, key-maximum threshold relative to the highest key maximum.
    pub mpm_k: f32,
    /// YIN: absolute threshold on the cumulative-mean-normalised difference.
    pub yin_threshold: f32,
}

impl Default for PitchConfig {
    fn default() -> Self {
        Self {
            sample_rate: crate::SAMPLE_RATE,
            min_freq: 60.0,
            max_freq: 1500.0,
            max_window: 2048,
            min_overlap: 0.25,
            min_overlap_samples: 96,
            mpm_k: 0.9,
            yin_threshold: 0.15,
        }
    }
}

/// Common interface of the pitch detectors.
pub trait PitchDetector: Send {
    /// Estimates the pitch of the most recent `min(x.len(), max_window)` samples of `x`.
    fn detect(&mut self, x: &[f32]) -> Option<PitchEstimate>;

    /// Shortest window (samples) on which a fundamental of `freq_hz` can be reported.
    fn min_window_for(&self, freq_hz: f32) -> usize;

    fn config(&self) -> &PitchConfig;
}

/// Vertex of the parabola through `(-1, a)`, `(0, b)`, `(1, c)`: returns (offset, value).
#[inline]
pub(crate) fn parabolic_peak(a: f32, b: f32, c: f32) -> (f32, f32) {
    let denom = a - 2.0 * b + c;
    if denom.abs() < 1e-12 {
        return (0.0, b);
    }
    let delta = (0.5 * (a - c) / denom).clamp(-0.5, 0.5);
    (delta, b - 0.25 * (a - c) * delta)
}

#[cfg(test)]
mod tests {
    use super::parabolic_peak;

    #[test]
    fn parabola_vertex_is_exact_for_quadratics() {
        // y = 2 - (x - 0.3)^2 sampled at -1, 0, 1.
        let f = |x: f32| 2.0 - (x - 0.3) * (x - 0.3);
        let (d, v) = parabolic_peak(f(-1.0), f(0.0), f(1.0));
        assert!((d - 0.3).abs() < 1e-6 && (v - 2.0).abs() < 1e-6);
    }
}
