//! Polyphonic chord verification: Hann-windowed FFT → interpolated spectral peaks →
//! (1) per-expected-note evidence, (2) energy left unexplained by the expected notes' harmonic
//! series, (3) a 12-bin chromagram for display.
//!
//! The game always knows which chord it expects, so this answers "was *this* chord played?"
//! rather than doing blind multi-pitch transcription.

use crate::fft::FftBank;
use crate::pitch::parabolic_peak;
use crate::{db_to_lin, note};

/// Most notes a chord can contain (one per string).
pub const MAX_CHORD_NOTES: usize = 6;
const MAX_PEAKS: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChordConfig {
    pub sample_rate: f32,
    /// Analysis window in samples; 4096 (85 ms) separates fourths and fifths down to E2.
    pub window: usize,
    /// Zero-padded FFT size (power of two ≥ window), for peak-frequency precision.
    pub fft_size: usize,
    /// Harmonics per expected note used to explain spectral peaks.
    pub harmonics: usize,
    /// Peak ↔ partial matching tolerance (cents), widened by 2 cents per harmonic rank for
    /// string inharmonicity.
    pub tolerance_cents: f32,
    /// A note is present when its fundamental peak is at least this loud relative to the
    /// strongest peak — or, when that fundamental is not resolved from a neighbouring chord tone,
    /// when its 2nd and 3rd harmonics are, provided no other expected note's harmonic series
    /// lands on them (the 5th harmonic of a root sits 14 cents from its major third, so shared
    /// harmonics cannot tell major from minor).
    pub presence_db: f32,
    /// Peaks below this level relative to the strongest are ignored.
    pub floor_db: f32,
    pub min_freq: f32,
    pub max_freq: f32,
    /// Accept when at most this fraction of peak energy is unexplained by the expected notes …
    pub max_foreign: f32,
    /// … and no single unexplained peak comes within this many dB of the strongest peak (an
    /// extra note a semitone from a chord tone merges with it at low frequencies, but its upper
    /// harmonics stand alone).
    pub foreign_peak_db: f32,
    /// Accept when at least this fraction of the expected notes is present.
    pub min_present: f32,
    /// Below this window RMS (dBFS) nothing is considered played.
    pub min_level_db: f32,
}

impl Default for ChordConfig {
    fn default() -> Self {
        Self {
            sample_rate: crate::SAMPLE_RATE,
            window: 4096,
            fft_size: 16384,
            harmonics: 20,
            tolerance_cents: 40.0,
            presence_db: -30.0,
            floor_db: -60.0,
            min_freq: 60.0,
            max_freq: 5000.0,
            max_foreign: 0.15,
            foreign_peak_db: -20.0,
            min_present: 1.0,
            min_level_db: -60.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChordMatch {
    pub accepted: bool,
    /// Per expected note (in the order given), whether it was found.
    pub present: [bool; MAX_CHORD_NOTES],
    pub n_expected: usize,
    /// Fraction of spectral-peak energy not explained by the expected notes' harmonics.
    pub foreign_ratio: f32,
    /// Level of the strongest unexplained peak relative to the strongest peak (dB).
    pub foreign_peak_db: f32,
    /// Normalised chromagram (C = 0), max bin = 1.
    pub chroma: [f32; 12],
}

impl ChordMatch {
    pub fn n_present(&self) -> usize {
        self.present[..self.n_expected].iter().filter(|&&p| p).count()
    }
}

pub struct ChordVerifier {
    cfg: ChordConfig,
    fft: FftBank,
    frame: Vec<f32>,
    mags: Vec<f32>,
    peaks: Vec<(f32, f32)>,
    level_db: f32,
}

impl ChordVerifier {
    pub fn new(cfg: ChordConfig) -> Self {
        assert!(cfg.fft_size.is_power_of_two() && cfg.fft_size >= cfg.window);
        Self {
            cfg,
            fft: FftBank::new(cfg.fft_size),
            frame: vec![0.0; cfg.window],
            mags: vec![0.0; cfg.fft_size / 2 + 1],
            peaks: Vec::with_capacity(MAX_PEAKS),
            level_db: -240.0,
        }
    }

    pub fn config(&self) -> &ChordConfig {
        &self.cfg
    }

    /// Spectral peaks (Hz, linear magnitude) of the last analysis.
    pub fn peaks(&self) -> &[(f32, f32)] {
        &self.peaks
    }

    /// Analyses the most recent `min(x.len(), window)` samples.
    pub fn analyze(&mut self, x: &[f32]) {
        let n = x.len().min(self.cfg.window);
        let x = &x[x.len() - n..];
        self.peaks.clear();
        let energy: f64 = x.iter().map(|&v| v as f64 * v as f64).sum();
        self.level_db = (10.0 * (energy / n.max(1) as f64 + 1e-24).log10()) as f32;
        if n < 64 || self.level_db < self.cfg.min_level_db {
            return;
        }
        let step = std::f32::consts::TAU / n as f32;
        for (i, (f, &v)) in self.frame[..n].iter_mut().zip(x).enumerate() {
            *f = v * (0.5 - 0.5 * (step * i as f32).cos());
        }
        let spec = self.fft.forward(&self.frame[..n], self.cfg.fft_size);
        for (m, c) in self.mags.iter_mut().zip(spec) {
            *m = c.norm();
        }

        let bin_hz = self.cfg.sample_rate / self.cfg.fft_size as f32;
        let lo = ((self.cfg.min_freq * 0.9 / bin_hz) as usize).max(2);
        let hi = ((self.cfg.max_freq / bin_hz) as usize).min(self.mags.len() - 2);
        let strongest = self.mags[lo..=hi].iter().fold(0f32, |a, &m| a.max(m));
        if strongest <= 0.0 {
            return;
        }
        let floor = strongest * db_to_lin(self.cfg.floor_db);
        for k in lo..=hi {
            let m = self.mags[k];
            if m > floor && m > self.mags[k - 1] && m >= self.mags[k + 1] {
                let (a, b, c) = (self.mags[k - 1].max(1e-20).ln(), m.ln(), self.mags[k + 1].max(1e-20).ln());
                let (d, v) = parabolic_peak(a, b, c);
                self.peaks.push(((k as f32 + d) * bin_hz, v.exp()));
                if self.peaks.len() == MAX_PEAKS {
                    break;
                }
            }
        }
    }

    /// Chromagram of the last analysis (peak energy folded onto pitch classes, max = 1).
    pub fn chroma(&self, a4_hz: f32) -> [f32; 12] {
        let mut chroma = [0f32; 12];
        for &(f, m) in &self.peaks {
            let midi = note::freq_to_midi(f, a4_hz).round() as i32;
            chroma[note::pitch_class(midi)] += m * m;
        }
        let max = chroma.iter().fold(0f32, |a, &v| a.max(v));
        if max > 0.0 {
            chroma.iter_mut().for_each(|v| *v /= max);
        }
        chroma
    }

    fn peak_near(&self, freq: f32, tol_cents: f32, min_mag: f32) -> bool {
        self.peaks
            .iter()
            .any(|&(f, m)| m >= min_mag && note::cents_between(f, freq).abs() <= tol_cents)
    }

    /// Checks the last `window` samples of `x` against the expected MIDI notes.
    pub fn verify(&mut self, x: &[f32], expected: &[i32], a4_hz: f32) -> ChordMatch {
        self.analyze(x);
        let n_expected = expected.len().min(MAX_CHORD_NOTES);
        let mut out = ChordMatch {
            accepted: false,
            present: [false; MAX_CHORD_NOTES],
            n_expected,
            foreign_ratio: 1.0,
            foreign_peak_db: 0.0,
            chroma: self.chroma(a4_hz),
        };
        if self.peaks.is_empty() || n_expected == 0 {
            return out;
        }
        let mut fundamentals = [0f32; MAX_CHORD_NOTES];
        for (f, &m) in fundamentals.iter_mut().zip(expected) {
            *f = note::midi_to_freq(m as f32, a4_hz);
        }
        let fundamentals = &fundamentals[..n_expected];

        let strongest = self.peaks.iter().fold(0f32, |a, p| a.max(p.1));
        let min_mag = strongest * db_to_lin(self.cfg.presence_db);
        let tol = self.cfg.tolerance_cents;
        let harmonic_of = |f: f32, f0: f32| {
            let h = (f / f0).round();
            h >= 1.0 && h <= self.cfg.harmonics as f32 && note::cents_between(f, h * f0).abs() <= tol + 2.0 * h
        };
        for (i, (present, &f0)) in out.present.iter_mut().zip(fundamentals).enumerate() {
            let own = |h: f32| {
                let f = h * f0;
                let shared = fundamentals.iter().enumerate().any(|(j, &g)| j != i && harmonic_of(f, g));
                !shared && self.peak_near(f, tol + 2.0 * h, min_mag)
            };
            *present = self.peak_near(f0, tol, min_mag) || (own(2.0) && own(3.0));
        }

        let (mut total, mut foreign, mut loudest_foreign) = (0f64, 0f64, 0f32);
        for &(f, m) in &self.peaks {
            let e = m as f64 * m as f64;
            total += e;
            let explained = fundamentals.iter().any(|&f0| harmonic_of(f, f0));
            if !explained {
                foreign += e;
                loudest_foreign = loudest_foreign.max(m);
            }
        }
        out.foreign_ratio = if total > 0.0 { (foreign / total) as f32 } else { 1.0 };
        out.foreign_peak_db = crate::lin_to_db(loudest_foreign / strongest);
        let present_ratio = out.n_present() as f32 / n_expected as f32;
        out.accepted = present_ratio >= self.cfg.min_present
            && out.foreign_ratio <= self.cfg.max_foreign
            && out.foreign_peak_db <= self.cfg.foreign_peak_db;
        out
    }
}
