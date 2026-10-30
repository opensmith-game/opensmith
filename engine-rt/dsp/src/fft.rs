//! FFT plumbing on top of `realfft`: every power-of-two size is planned up front so that the
//! correlation routines never allocate.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

const MIN_FFT: usize = 64;

/// Bank of real FFT plans for sizes `MIN_FFT ..= max_size` (powers of two) plus work buffers.
pub struct FftBank {
    fwd: Vec<Arc<dyn RealToComplex<f32>>>,
    inv: Vec<Arc<dyn ComplexToReal<f32>>>,
    time_a: Vec<f32>,
    time_b: Vec<f32>,
    spec_a: Vec<Complex32>,
    spec_b: Vec<Complex32>,
    scratch: Vec<Complex32>,
    max_size: usize,
}

impl FftBank {
    /// Plans every size up to `max_size` (rounded up to a power of two).
    pub fn new(max_size: usize) -> Self {
        let max_size = max_size.next_power_of_two().max(MIN_FFT);
        let mut planner = RealFftPlanner::<f32>::new();
        let (mut fwd, mut inv) = (Vec::new(), Vec::new());
        let mut scratch_len = 0;
        let mut size = MIN_FFT;
        while size <= max_size {
            let f = planner.plan_fft_forward(size);
            let i = planner.plan_fft_inverse(size);
            scratch_len = scratch_len.max(f.get_scratch_len()).max(i.get_scratch_len());
            fwd.push(f);
            inv.push(i);
            size *= 2;
        }
        let zero = Complex32::new(0.0, 0.0);
        Self {
            fwd,
            inv,
            time_a: vec![0.0; max_size],
            time_b: vec![0.0; max_size],
            spec_a: vec![zero; max_size / 2 + 1],
            spec_b: vec![zero; max_size / 2 + 1],
            scratch: vec![zero; scratch_len],
            max_size,
        }
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }

    fn plan_index(&self, size: usize) -> usize {
        debug_assert!(size.is_power_of_two() && (MIN_FFT..=self.max_size).contains(&size));
        (size / MIN_FFT).trailing_zeros() as usize
    }

    /// Linear autocorrelation `r[τ] = Σ_{j=0}^{n-1-τ} x[j]·x[j+τ]` for `τ ∈ 0..n`, `n = x.len()`.
    /// Requires `2·n ≤ max_size`.
    pub fn autocorr(&mut self, x: &[f32], r: &mut [f32]) {
        let n = x.len();
        let size = (2 * n).next_power_of_two().max(MIN_FFT);
        assert!(size <= self.max_size && r.len() >= n, "autocorr window too large");
        let k = self.plan_index(size);
        let bins = size / 2 + 1;

        let time = &mut self.time_a[..size];
        time[..n].copy_from_slice(x);
        time[n..].fill(0.0);
        let spec = &mut self.spec_a[..bins];
        let fwd = &self.fwd[k];
        fwd.process_with_scratch(time, spec, &mut self.scratch[..fwd.get_scratch_len()])
            .expect("planned FFT");
        for c in spec.iter_mut() {
            *c = Complex32::new(c.norm_sqr(), 0.0);
        }
        let inv = &self.inv[k];
        inv.process_with_scratch(spec, time, &mut self.scratch[..inv.get_scratch_len()])
            .expect("planned FFT");
        let scale = 1.0 / size as f32;
        for (dst, src) in r[..n].iter_mut().zip(time.iter()) {
            *dst = src * scale;
        }
    }

    /// Prefix cross-correlation `c[τ] = Σ_{j=0}^{w-1} x[j]·x[j+τ]` for `τ ∈ 0..=x.len()-w`.
    /// Requires `x.len() ≤ max_size`.
    pub fn prefix_xcorr(&mut self, x: &[f32], w: usize, c: &mut [f32]) {
        let n = x.len();
        assert!(w <= n);
        let lags = n - w + 1;
        let size = n.next_power_of_two().max(MIN_FFT);
        assert!(size <= self.max_size && c.len() >= lags, "xcorr window too large");
        let k = self.plan_index(size);
        let bins = size / 2 + 1;

        let ta = &mut self.time_a[..size];
        ta[..n].copy_from_slice(x);
        ta[n..].fill(0.0);
        let tb = &mut self.time_b[..size];
        tb[..w].copy_from_slice(&x[..w]);
        tb[w..].fill(0.0);
        let fwd = &self.fwd[k];
        let scratch = &mut self.scratch[..fwd.get_scratch_len()];
        fwd.process_with_scratch(ta, &mut self.spec_a[..bins], scratch).expect("planned FFT");
        fwd.process_with_scratch(tb, &mut self.spec_b[..bins], scratch).expect("planned FFT");
        for (a, b) in self.spec_a[..bins].iter_mut().zip(&self.spec_b[..bins]) {
            *a *= b.conj();
        }
        // DC and Nyquist bins of A·conj(B) are real for real inputs; enforce it exactly.
        self.spec_a[0].im = 0.0;
        self.spec_a[bins - 1].im = 0.0;
        let inv = &self.inv[k];
        let ta = &mut self.time_a[..size];
        inv.process_with_scratch(
            &mut self.spec_a[..bins],
            ta,
            &mut self.scratch[..inv.get_scratch_len()],
        )
        .expect("planned FFT");
        let scale = 1.0 / size as f32;
        for (dst, src) in c[..lags].iter_mut().zip(ta.iter()) {
            *dst = src * scale;
        }
    }

    /// Forward real FFT of `x` zero-padded to `size`; returns the `size/2+1` bins.
    pub fn forward(&mut self, x: &[f32], size: usize) -> &[Complex32] {
        assert!(x.len() <= size && size <= self.max_size);
        let k = self.plan_index(size);
        let bins = size / 2 + 1;
        let time = &mut self.time_a[..size];
        time[..x.len()].copy_from_slice(x);
        time[x.len()..].fill(0.0);
        let fwd = &self.fwd[k];
        fwd.process_with_scratch(time, &mut self.spec_a[..bins], &mut self.scratch[..fwd.get_scratch_len()])
            .expect("planned FFT");
        &self.spec_a[..bins]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive_autocorr(x: &[f32]) -> Vec<f32> {
        (0..x.len()).map(|t| (0..x.len() - t).map(|j| x[j] * x[j + t]).sum()).collect()
    }

    #[test]
    fn autocorr_matches_naive() {
        let x: Vec<f32> = (0..300).map(|i| ((i * 37 % 101) as f32 / 50.0 - 1.0) * (i as f32 * 0.07).sin()).collect();
        let mut bank = FftBank::new(1024);
        let mut r = vec![0.0; x.len()];
        bank.autocorr(&x, &mut r);
        for (a, b) in r.iter().zip(naive_autocorr(&x)) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    #[test]
    fn prefix_xcorr_matches_naive() {
        let x: Vec<f32> = (0..500).map(|i| (i as f32 * 0.123).sin() + 0.3 * (i as f32 * 0.71).cos()).collect();
        let w = 200;
        let mut bank = FftBank::new(1024);
        let mut c = vec![0.0; x.len() - w + 1];
        bank.prefix_xcorr(&x, w, &mut c);
        for (t, got) in c.iter().enumerate() {
            let want: f32 = (0..w).map(|j| x[j] * x[j + t]).sum();
            assert!((got - want).abs() < 2e-3, "lag {t}: {got} vs {want}");
        }
    }
}
