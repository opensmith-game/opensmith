//! Speaker-cabinet impulse responses and a zero-latency partitioned convolver.
//!
//! Convolver = direct FIR for the first `B` taps (no latency for any block size) + uniformly
//! partitioned overlap-save FFT convolution (partition `B`, FFT `2B`) for taps `B..`. Taps
//! `B + jB ..` act on input delayed by `(j+1)·B`, so the tail for output block `m` only needs
//! input blocks `≤ m−1`: it is computed the instant block `m−1` completes.

use std::path::Path;
use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use super::biquad::Biquad;
use super::resample::resample;

#[derive(Debug, thiserror::Error)]
pub enum IrError {
    #[error("cannot read impulse response: {0}")]
    Wav(#[from] hound::Error),
    #[error("impulse response is empty or silent")]
    Empty,
}

/// A mono impulse response at the engine sample rate, peak-normalised to 0 dB of gain.
#[derive(Clone, Debug)]
pub struct CabinetIr {
    pub samples: Vec<f32>,
    pub sample_rate: f32,
    pub name: String,
}

impl CabinetIr {
    /// Loads a `.wav` IR (any bit depth / channel count; channels are averaged), resamples it to
    /// `sample_rate`, trims trailing silence (−80 dB) and caps it at `max_ms`.
    pub fn from_wav(path: impl AsRef<Path>, sample_rate: f32, max_ms: f32) -> Result<Self, IrError> {
        let path = path.as_ref();
        let (data, sr) = crate::wav::read_mono(path)?;
        let mut samples = resample(&data, sr as f32, sample_rate, 32);
        let peak = samples.iter().fold(0f32, |a, &v| a.max(v.abs()));
        if peak <= 0.0 {
            return Err(IrError::Empty);
        }
        let floor = peak * 1e-4;
        let end = samples.iter().rposition(|v| v.abs() > floor).map_or(0, |i| i + 1);
        samples.truncate(end.min((max_ms * 1e-3 * sample_rate) as usize).max(1));
        let name = path.file_stem().map_or_else(|| "ir".into(), |s| s.to_string_lossy().into_owned());
        Ok(Self::normalized(samples, sample_rate, name))
    }

    /// Built-in 4×12 closed-back cabinet approximation: resonance at 110 Hz, low-mid scoop,
    /// presence bump, steep roll-off above 5 kHz. 2048 taps.
    pub fn builtin_4x12(sample_rate: f32) -> Self {
        let sr = sample_rate;
        let mut filters = [
            Biquad::highpass(sr, 70.0, 0.8),
            Biquad::peaking(sr, 110.0, 1.1, 5.0),
            Biquad::peaking(sr, 450.0, 0.9, -3.5),
            Biquad::peaking(sr, 2200.0, 1.3, 4.0),
            Biquad::lowpass(sr, 5200.0, 0.9),
            Biquad::lowpass(sr, 6500.0, 0.6),
        ];
        let samples = (0..2048)
            .map(|i| {
                let x = if i == 0 { 1.0 } else { 0.0 };
                filters.iter_mut().fold(x, |s, f| f.process(s))
            })
            .collect();
        Self::normalized(samples, sample_rate, "builtin-4x12".into())
    }

    /// Scales the IR so that its magnitude response peaks at exactly 0 dB.
    fn normalized(mut samples: Vec<f32>, sample_rate: f32, name: String) -> Self {
        let size = (samples.len() * 4).next_power_of_two().max(4096);
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let mut time = vec![0.0; size];
        time[..samples.len()].copy_from_slice(&samples);
        let mut spec = fft.make_output_vec();
        fft.process(&mut time, &mut spec).expect("planned FFT");
        let peak = spec.iter().fold(0f32, |a, c| a.max(c.norm()));
        if peak > 0.0 {
            samples.iter_mut().for_each(|v| *v /= peak);
        }
        Self { samples, sample_rate, name }
    }
}

pub struct Convolver {
    b: usize,
    /// First `b` taps, reversed, for the direct FIR.
    head_rev: Vec<f32>,
    /// Input history written twice (`p` and `p + b`) so the last `b` samples are contiguous.
    hist: Vec<f32>,
    hist_pos: usize,
    /// Spectra of the tail partitions, scaled by 1/(2b), `parts × (b+1)`.
    parts: Vec<Complex32>,
    n_parts: usize,
    /// Frequency-domain delay line of input-block spectra, same layout, circular.
    fdl: Vec<Complex32>,
    fdl_pos: usize,
    /// [previous block | current block].
    in_buf: Vec<f32>,
    in_fill: usize,
    tail: Vec<f32>,
    fft_fwd: Arc<dyn RealToComplex<f32>>,
    fft_inv: Arc<dyn ComplexToReal<f32>>,
    time: Vec<f32>,
    spec: Vec<Complex32>,
    acc: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl Convolver {
    /// `block` is the partition size; 64 matches the CoreAudio buffer, but any call size works.
    pub fn new(ir: &[f32], block: usize) -> Self {
        assert!(block.is_power_of_two() && !ir.is_empty());
        let b = block;
        let bins = b + 1;
        let mut head_rev = vec![0.0; b];
        for (i, &h) in ir.iter().take(b).enumerate() {
            head_rev[b - 1 - i] = h;
        }
        let mut planner = RealFftPlanner::<f32>::new();
        let fft_fwd = planner.plan_fft_forward(2 * b);
        let fft_inv = planner.plan_fft_inverse(2 * b);
        let scratch_len = fft_fwd.get_scratch_len().max(fft_inv.get_scratch_len());
        let zero = Complex32::new(0.0, 0.0);

        let tail_taps = ir.len().saturating_sub(b);
        let n_parts = tail_taps.div_ceil(b);
        let mut parts = vec![zero; n_parts * bins];
        let mut time = vec![0.0; 2 * b];
        let mut scratch = vec![zero; scratch_len];
        let scale = 1.0 / (2 * b) as f32;
        for j in 0..n_parts {
            time.fill(0.0);
            let from = b + j * b;
            let to = (from + b).min(ir.len());
            for (t, &h) in time.iter_mut().zip(&ir[from..to]) {
                *t = h * scale;
            }
            fft_fwd
                .process_with_scratch(&mut time, &mut parts[j * bins..(j + 1) * bins], &mut scratch)
                .expect("planned FFT");
        }

        Self {
            b,
            head_rev,
            hist: vec![0.0; 2 * b],
            hist_pos: 0,
            parts,
            n_parts,
            fdl: vec![zero; n_parts * bins],
            fdl_pos: 0,
            in_buf: vec![0.0; 2 * b],
            in_fill: 0,
            tail: vec![0.0; b],
            fft_fwd,
            fft_inv,
            time,
            spec: vec![zero; bins],
            acc: vec![zero; bins],
            scratch,
        }
    }

    pub fn ir_len(&self) -> usize {
        self.b + self.n_parts * self.b
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let b = self.b;
        let p = self.hist_pos;
        self.hist[p] = x;
        self.hist[p + b] = x;
        let window = &self.hist[p + 1..p + 1 + b];
        let mut y: f32 = self.head_rev.iter().zip(window).map(|(h, v)| h * v).sum();
        self.hist_pos = (p + 1) % b;

        if self.n_parts > 0 {
            y += self.tail[self.in_fill];
            self.in_buf[b + self.in_fill] = x;
            self.in_fill += 1;
            if self.in_fill == b {
                self.compute_tail();
                self.in_fill = 0;
            }
        }
        y
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for v in buf {
            *v = self.process_sample(*v);
        }
    }

    fn compute_tail(&mut self) {
        let (b, bins) = (self.b, self.b + 1);
        self.time.copy_from_slice(&self.in_buf);
        self.in_buf.copy_within(b.., 0);

        self.fdl_pos = (self.fdl_pos + self.n_parts - 1) % self.n_parts;
        let slot = self.fdl_pos * bins;
        self.fft_fwd
            .process_with_scratch(&mut self.time, &mut self.fdl[slot..slot + bins], &mut self.scratch)
            .expect("planned FFT");

        self.acc.fill(Complex32::new(0.0, 0.0));
        for j in 0..self.n_parts {
            let x = &self.fdl[((self.fdl_pos + j) % self.n_parts) * bins..][..bins];
            let h = &self.parts[j * bins..(j + 1) * bins];
            for ((a, x), h) in self.acc.iter_mut().zip(x).zip(h) {
                *a += x * h;
            }
        }
        self.spec.copy_from_slice(&self.acc);
        self.spec[0].im = 0.0;
        self.spec[b].im = 0.0;
        self.fft_inv
            .process_with_scratch(&mut self.spec, &mut self.time, &mut self.scratch)
            .expect("planned FFT");
        self.tail.copy_from_slice(&self.time[b..]);
    }
}
