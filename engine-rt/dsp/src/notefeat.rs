//! What the learned note detector (`notenet`) sees of the guitar: a log-frequency spectrogram,
//! three bins per semitone from MIDI `LOW_MIDI` up, a frame every 10 ms over 85 ms windows,
//! log-compressed. The same code makes the training data and runs in the game, so both see
//! exactly the same thing.

use crate::fft::FftBank;

/// Analysis window (samples at 48 kHz): 85 ms.
pub const WINDOW: usize = 4096;
/// Zero-padded FFT size, for finer bins at low pitches.
pub const FFT: usize = 8192;
/// Frame period (samples): 10 ms.
pub const HOP: usize = 480;
/// Pitch of the first bin (MIDI).
pub const LOW_MIDI: f32 = 26.0;
/// Bins per semitone.
pub const PER_SEMITONE: usize = 3;
/// Bins in a frame: six octaves, MIDI 26 to 98.
pub const BINS: usize = 72 * PER_SEMITONE;
pub const SAMPLE_RATE: f32 = 48_000.0;

/// Frequency (Hz) of bin `b`.
pub fn bin_hz(b: usize) -> f32 {
    440.0 * 2f32.powf((LOW_MIDI + b as f32 / PER_SEMITONE as f32 - 69.0) / 12.0)
}

pub struct NoteFeatures {
    fft: FftBank,
    window: Vec<f32>,
    frame: Vec<f32>,
    mags: Vec<f32>,
    /// Per output bin: the FFT bins it spans (first, last), and the fractional position of its
    /// centre (where a single FFT bin is wider than it).
    spans: Vec<(usize, usize, f32)>,
}

impl Default for NoteFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl NoteFeatures {
    pub fn new() -> Self {
        let step = std::f32::consts::TAU / WINDOW as f32;
        let window: Vec<f32> = (0..WINDOW).map(|i| 0.5 - 0.5 * (step * i as f32).cos()).collect();
        let hz = SAMPLE_RATE / FFT as f32;
        let edge = 2f32.powf(1.0 / (24.0 * PER_SEMITONE as f32));
        let spans = (0..BINS)
            .map(|b| {
                let f = bin_hz(b);
                let lo = ((f / edge) / hz).ceil() as usize;
                let hi = ((f * edge) / hz).floor() as usize;
                (lo, hi.min(FFT / 2), f / hz)
            })
            .collect();
        NoteFeatures { fft: FftBank::new(FFT), window, frame: vec![0.0; WINDOW], mags: vec![0.0; FFT / 2 + 1], spans }
    }

    /// The frame centred on sample `centre` of `x` (zeros past its ends), into `out` (`BINS`).
    pub fn frame(&mut self, x: &[f32], centre: usize, out: &mut [f32]) {
        let start = centre as isize - (WINDOW / 2) as isize;
        for (i, (f, w)) in self.frame.iter_mut().zip(&self.window).enumerate() {
            let j = start + i as isize;
            *f = if j >= 0 && (j as usize) < x.len() { x[j as usize] * w } else { 0.0 };
        }
        let spec = self.fft.forward(&self.frame, FFT);
        // Amplitude of a full-scale sine: the window's sum is WINDOW / 2.
        let norm = 2.0 / (WINDOW as f32 / 2.0);
        for (m, c) in self.mags.iter_mut().zip(spec) {
            *m = c.norm() * norm;
        }
        for (o, &(lo, hi, centre)) in out.iter_mut().zip(&self.spans) {
            let amp = if hi > lo {
                self.mags[lo..=hi].iter().fold(0.0f32, |a, &v| a.max(v))
            } else {
                let k = centre.floor() as usize;
                let t = centre - k as f32;
                self.mags[k] * (1.0 - t) + self.mags[(k + 1).min(FFT / 2)] * t
            };
            *o = (1.0 + 1000.0 * amp).ln();
        }
    }

    /// Frames every `HOP` samples over `x`, frame k centred on sample k·HOP: frames × `BINS`,
    /// row-major.
    pub fn spectrogram(&mut self, x: &[f32]) -> Vec<f32> {
        let frames = x.len() / HOP + 1;
        let mut out = vec![0.0; frames * BINS];
        for (k, row) in out.chunks_mut(BINS).enumerate() {
            self.frame(x, k * HOP, row);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_lands_in_its_bin() {
        let x: Vec<f32> = (0..48_000).map(|i| 0.25 * (std::f32::consts::TAU * 440.0 * i as f32 / 48_000.0).sin()).collect();
        let mut nf = NoteFeatures::new();
        let mut row = vec![0.0; BINS];
        nf.frame(&x, 24_000, &mut row);
        let top = row.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(top, (69 - 26) * PER_SEMITONE, "A4 in the bin of MIDI 69");
        assert!((row[top] - (1.0 + 1000.0 * 0.25f32).ln()).abs() < 0.2, "amplitude {}", row[top]);
        // A low E peaks within a semitone of its pitch: 85 ms cannot split low semitones, the
        // detector tells them apart by their harmonics.
        let e: Vec<f32> = (0..48_000).map(|i| 0.25 * (std::f32::consts::TAU * 82.41 * i as f32 / 48_000.0).sin()).collect();
        nf.frame(&e, 24_000, &mut row);
        let top = row.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0 as i32;
        assert!((top - ((40 - 26) * PER_SEMITONE) as i32).abs() <= PER_SEMITONE as i32, "E2 peaks at bin {top}");
    }
}
