//! McLeod Pitch Method (McLeod & Wyvill, "A smarter way to find pitch", ICMC 2005).
//!
//! NSDF  n'(τ) = 2·r'(τ) / m'(τ)  with  r'(τ) = Σ x_j·x_{j+τ}  and  m'(τ) = Σ (x_j² + x_{j+τ}²),
//! both summed over the overlap j ∈ [0, W−τ). r' comes from one FFT autocorrelation, m' from an
//! O(W) recursion. The pitch is the first "key maximum" (highest NSDF value of each positive lobe)
//! reaching k × the highest key maximum, refined by parabolic interpolation.

use super::{parabolic_peak, PitchConfig, PitchDetector, PitchEstimate};
use crate::fft::FftBank;

const MAX_KEYS: usize = 128;
/// A lobe still open at the window edge only counts once the NSDF has fallen this much past
/// its peak: a ripple on the rising flank of the true peak must not pass for a maximum.
const OPEN_LOBE_DROP: f32 = 0.05;
/// Lag margin (fraction of the period) needed past a peak to observe that fall.
const OPEN_LOBE_MARGIN: f32 = 0.06;

pub struct Mpm {
    cfg: PitchConfig,
    fft: FftBank,
    r: Vec<f32>,
    nsdf: Vec<f32>,
    nsdf_len: usize,
}

impl Mpm {
    pub fn new(cfg: PitchConfig) -> Self {
        Self {
            cfg,
            fft: FftBank::new(2 * cfg.max_window),
            r: vec![0.0; cfg.max_window],
            nsdf: vec![0.0; cfg.max_window],
            nsdf_len: 0,
        }
    }

    /// NSDF computed by the last call to `detect` (lags `0..len`), for diagnostics.
    pub fn nsdf(&self) -> &[f32] {
        &self.nsdf[..self.nsdf_len]
    }

    fn tau_min(&self) -> usize {
        ((self.cfg.sample_rate / self.cfg.max_freq).floor() as usize).max(2)
    }

    /// Largest lag searched on an `n`-sample window.
    fn tau_max(&self, n: usize) -> usize {
        let by_overlap = ((n as f32 / (1.0 + self.cfg.min_overlap)).floor() as usize)
            .min(n.saturating_sub(self.cfg.min_overlap_samples));
        let by_freq = (self.cfg.sample_rate / self.cfg.min_freq).ceil() as usize + 2;
        by_overlap.min(by_freq).min(n.saturating_sub(1))
    }
}

impl PitchDetector for Mpm {
    fn detect(&mut self, x: &[f32]) -> Option<PitchEstimate> {
        let n = x.len().min(self.cfg.max_window);
        let x = &x[x.len() - n..];
        let tau_min = self.tau_min();
        let tau_max = self.tau_max(n);
        self.nsdf_len = 0;
        if tau_max < tau_min + 3 {
            return None;
        }

        self.fft.autocorr(x, &mut self.r[..n]);
        let r0 = self.r[0];
        if r0 <= 1e-10 {
            return None;
        }

        // m'(τ) = m'(τ−1) − x[τ−1]² − x[n−τ]², accumulated in f64 to avoid drift.
        let nsdf = &mut self.nsdf;
        nsdf[0] = 1.0;
        let mut m = 2.0 * r0 as f64;
        let floor = 1e-9 * m;
        for tau in 1..=tau_max {
            let a = x[tau - 1] as f64;
            let b = x[n - tau] as f64;
            m -= a * a + b * b;
            nsdf[tau] = if m > floor { (2.0 * self.r[tau] as f64 / m) as f32 } else { 0.0 };
        }
        self.nsdf_len = tau_max + 1;

        // Key maxima: highest point of each positive lobe after the zero-lag lobe.
        let mut keys = [(0usize, 0f32); MAX_KEYS];
        let mut n_keys = 0;
        let mut tau = 1;
        while tau <= tau_max && nsdf[tau] > 0.0 {
            tau += 1;
        }
        let mut lobe: Option<(usize, f32)> = None;
        while tau <= tau_max {
            let v = nsdf[tau];
            if v > 0.0 {
                match lobe {
                    Some((_, best)) if v <= best => {}
                    _ => lobe = Some((tau, v)),
                }
            } else if let Some((t, best)) = lobe.take() {
                if t >= tau_min && n_keys < MAX_KEYS {
                    keys[n_keys] = (t, best);
                    n_keys += 1;
                }
            }
            tau += 1;
        }
        // A lobe still open at the window edge only counts if its peak is clearly behind us:
        // otherwise the true maximum may lie beyond the searchable lags.
        if let Some((t, best)) = lobe {
            let tail_min = nsdf[t..=tau_max].iter().fold(best, |m, &v| m.min(v));
            if t >= tau_min && t + 2 <= tau_max && best - tail_min >= OPEN_LOBE_DROP && n_keys < MAX_KEYS {
                keys[n_keys] = (t, best);
                n_keys += 1;
            }
        }
        if n_keys == 0 {
            return None;
        }

        let highest = keys[..n_keys].iter().fold(f32::MIN, |acc, k| acc.max(k.1));
        let threshold = self.cfg.mpm_k * highest;
        let &(t, _) = keys[..n_keys].iter().find(|k| k.1 >= threshold)?;
        let (delta, peak) = if t < tau_max {
            parabolic_peak(nsdf[t - 1], nsdf[t], nsdf[t + 1])
        } else {
            (0.0, nsdf[t])
        };
        let period = t as f32 + delta;
        Some(PitchEstimate {
            freq_hz: self.cfg.sample_rate / period,
            period,
            clarity: peak.clamp(0.0, 1.0),
        })
    }

    fn min_window_for(&self, freq_hz: f32) -> usize {
        let period = self.cfg.sample_rate / freq_hz;
        let lag = period * (1.0 + OPEN_LOBE_MARGIN) + 3.0;
        let relative = ((1.0 + self.cfg.min_overlap) * lag).ceil() as usize;
        relative.max(lag.ceil() as usize + self.cfg.min_overlap_samples)
    }

    fn config(&self) -> &PitchConfig {
        &self.cfg
    }
}
