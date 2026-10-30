//! YIN (de Cheveigné & Kawahara, JASA 2002), FFT-accelerated.
//!
//! With a buffer of `n` samples and maximum lag `τmax = n/2`, the integration window is
//! `W = n − τmax`; d(τ) = e(0) + e(τ) − 2·c(τ) where e is the sliding energy and c a prefix
//! cross-correlation computed by FFT. Then the cumulative-mean-normalised difference, absolute
//! threshold, local-minimum descent and parabolic refinement on d(τ).

use super::{parabolic_peak, PitchConfig, PitchDetector, PitchEstimate};
use crate::fft::FftBank;

pub struct Yin {
    cfg: PitchConfig,
    fft: FftBank,
    c: Vec<f32>,
    d: Vec<f32>,
    cmndf: Vec<f32>,
}

impl Yin {
    pub fn new(cfg: PitchConfig) -> Self {
        let lags = cfg.max_window / 2 + 2;
        Self {
            cfg,
            fft: FftBank::new(cfg.max_window),
            c: vec![0.0; lags],
            d: vec![0.0; lags],
            cmndf: vec![0.0; lags],
        }
    }

    fn tau_min(&self) -> usize {
        ((self.cfg.sample_rate / self.cfg.max_freq).floor() as usize).max(2)
    }
}

impl PitchDetector for Yin {
    fn detect(&mut self, x: &[f32]) -> Option<PitchEstimate> {
        let n = x.len().min(self.cfg.max_window);
        let x = &x[x.len() - n..];
        let tau_min = self.tau_min();
        let by_freq = (self.cfg.sample_rate / self.cfg.min_freq).ceil() as usize + 2;
        let tau_max = (n / 2).min(by_freq).min(n.saturating_sub(self.cfg.min_overlap_samples));
        if tau_max < tau_min + 3 {
            return None;
        }
        let w = n - tau_max;

        self.fft.prefix_xcorr(x, w, &mut self.c[..=tau_max]);
        let e0: f64 = x[..w].iter().map(|&v| v as f64 * v as f64).sum();
        if e0 <= 1e-10 {
            return None;
        }

        let (d, cm) = (&mut self.d, &mut self.cmndf);
        d[0] = 0.0;
        cm[0] = 1.0;
        let mut e_tau = e0;
        let mut running = 0f64;
        for tau in 1..=tau_max {
            let enter = x[tau + w - 1] as f64;
            let leave = x[tau - 1] as f64;
            e_tau += enter * enter - leave * leave;
            let dt = (e0 + e_tau - 2.0 * self.c[tau] as f64).max(0.0);
            d[tau] = dt as f32;
            running += dt;
            cm[tau] = if running > 0.0 { (dt * tau as f64 / running) as f32 } else { 1.0 };
        }

        // First region under the absolute threshold; its deepest point is the period (high
        // partials ripple the dip, so the first local minimum may sit a few samples early).
        let mut start = tau_min;
        while start < tau_max && cm[start] >= self.cfg.yin_threshold {
            start += 1;
        }
        if start >= tau_max {
            return None;
        }
        let mut end = start;
        while end < tau_max && cm[end + 1] < self.cfg.yin_threshold {
            end += 1;
        }
        let mut tau = start;
        for t in start..=end {
            if cm[t] < cm[tau] {
                tau = t;
            }
        }
        // The dip must be closed: a minimum on the last searchable lag may continue beyond it.
        if end >= tau_max || tau + 1 >= tau_max {
            return None;
        }
        // Refine on −d (a maximum of −d is a minimum of d).
        let (delta, _) = parabolic_peak(-d[tau - 1], -d[tau], -d[tau + 1]);
        let period = tau as f32 + delta;
        Some(PitchEstimate {
            freq_hz: self.cfg.sample_rate / period,
            period,
            clarity: (1.0 - cm[tau]).clamp(0.0, 1.0),
        })
    }

    fn min_window_for(&self, freq_hz: f32) -> usize {
        let lag = self.cfg.sample_rate / freq_hz + 3.0;
        ((2.0 * lag).ceil() as usize).max(lag.ceil() as usize + self.cfg.min_overlap_samples)
    }

    fn config(&self) -> &PitchConfig {
        &self.cfg
    }
}
