//! Deterministic guitar-like test material: sines, additive plucked strings (pluck- and
//! pickup-position combs, stiffness inharmonicity, per-partial decay, pick noise),
//! Karplus–Strong strings, strums and noise. Seeded PCG32, identical on every run and platform.

use std::f64::consts::{PI, TAU};

/// PCG32 (O'Neill), XSH-RR variant.
#[derive(Clone, Debug)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

impl Pcg32 {
    pub fn new(seed: u64) -> Self {
        let mut r = Self { state: 0, inc: seed.wrapping_mul(2) | 1 };
        r.next_u32();
        r.state = r.state.wrapping_add(0x853c_49e6_748f_ea9b ^ seed);
        r.next_u32();
        r
    }

    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        xorshifted.rotate_right((old >> 59) as u32)
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / (1u32 << 24) as f32)
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.uniform()
    }

    /// Standard normal (Box–Muller).
    pub fn gauss(&mut self) -> f32 {
        let u1 = self.uniform().max(1e-7);
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    }
}

pub fn sine(freq: f32, sr: f32, secs: f32, amp: f32) -> Vec<f32> {
    let n = (secs * sr) as usize;
    let w = TAU * freq as f64 / sr as f64;
    (0..n).map(|i| amp * (w * i as f64).sin() as f32).collect()
}

pub fn white_noise(rng: &mut Pcg32, n: usize, rms: f32) -> Vec<f32> {
    (0..n).map(|_| rms * rng.gauss()).collect()
}

/// Adds `src × gain` into `dst` at `offset`, growing `dst` as needed.
pub fn mix_into(dst: &mut Vec<f32>, src: &[f32], offset: usize, gain: f32) {
    if dst.len() < offset + src.len() {
        dst.resize(offset + src.len(), 0.0);
    }
    for (d, s) in dst[offset..].iter_mut().zip(src) {
        *d += s * gain;
    }
}

/// Additive model of a plucked steel string seen through a magnetic pickup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pluck {
    pub freq: f32,
    /// Upper bound of the peak amplitude.
    pub amp: f32,
    pub secs: f32,
    /// Plucking point as a fraction of the string length from the bridge (comb on partials).
    pub pluck_pos: f32,
    /// Pickup position (same convention); 0 disables the pickup comb.
    pub pickup_pos: f32,
    /// 60 dB decay time of the fundamental; partial h decays (1 + 0.04 (h−1)²) times faster.
    pub t60: f32,
    /// Stiffness coefficient B: f_h = h·f0·√(1 + B h²).
    pub inharmonicity: f32,
    /// Level of the plectrum click (0..1).
    pub pick_noise: f32,
    /// Spectral tilt exponent: 1 = plectrum, 2 = soft finger.
    pub tilt: f32,
    pub max_partial_hz: f32,
}

impl Pluck {
    pub fn new(freq: f32) -> Self {
        Self {
            freq,
            amp: 0.5,
            secs: 1.0,
            pluck_pos: 0.2,
            pickup_pos: 0.0,
            t60: 3.0,
            inharmonicity: 8e-5,
            pick_noise: 0.3,
            tilt: 1.0,
            max_partial_hz: 8000.0,
        }
    }

    pub fn render(&self, sr: f32, rng: &mut Pcg32) -> Vec<f32> {
        let n = (self.secs * sr) as usize;
        let mut out = vec![0f64; n];
        let mut norm = 0f64;
        let limit = (0.45 * sr).min(self.max_partial_hz) as f64;
        for h in 1..10_000 {
            let hf = h as f64;
            let f = self.freq as f64 * hf * (1.0 + self.inharmonicity as f64 * hf * hf).sqrt();
            if f >= limit {
                break;
            }
            let mut a = (PI * hf * self.pluck_pos as f64).sin().abs() / hf.powf(self.tilt as f64);
            if self.pickup_pos > 0.0 {
                a *= (PI * hf * self.pickup_pos as f64).sin().abs();
            }
            if a < 1e-5 {
                continue;
            }
            norm += a;
            let t60 = self.t60 as f64 / (1.0 + 0.04 * (hf - 1.0).powi(2));
            let r = (-(1000f64.ln()) / (t60 * sr as f64)).exp();
            let w = TAU * f / sr as f64;
            // Damped sine a·rⁱ·sin(w i) by recurrence.
            let (c, r2) = (2.0 * r * w.cos(), r * r);
            let (mut s2, mut s1) = (0.0, a * r * w.sin());
            if n > 1 {
                out[1] += s1;
            }
            for v in out.iter_mut().skip(2) {
                let s = c * s1 - r2 * s2;
                *v += s;
                s2 = s1;
                s1 = s;
            }
        }
        let scale = if norm > 0.0 { self.amp as f64 / norm } else { 0.0 };
        let mut y: Vec<f32> = out.iter().map(|v| (v * scale) as f32).collect();

        // Plectrum click: differentiated, fast-decaying noise burst.
        if self.pick_noise > 0.0 {
            let g = self.pick_noise * self.amp * 0.25;
            let tau = 0.0015 * sr;
            let mut prev = 0.0;
            for (i, v) in y.iter_mut().take((0.006 * sr) as usize).enumerate() {
                let e = g * rng.gauss() * (-(i as f32) / tau).exp();
                *v += 0.5 * (e - prev);
                prev = e;
            }
        }
        y
    }
}

/// Karplus–Strong string (Jaffe–Smith extensions: two-point loss filter, allpass fine tuning).
/// The loop starts from the displacement of a string plucked at `pluck_pos` (a triangle, whose
/// harmonics fall as sin(πhβ)/h²) plus `noise` × white noise for the plectrum texture. Pure
/// noise excitation would give each harmonic a random amplitude — not how a string behaves.
#[allow(clippy::too_many_arguments)]
pub fn karplus_strong(
    freq: f32,
    sr: f32,
    secs: f32,
    amp: f32,
    pluck_pos: f32,
    t60: f32,
    noise: f32,
    rng: &mut Pcg32,
) -> Vec<f32> {
    let n = (secs * sr) as usize;
    let period = sr as f64 / freq as f64;
    // Loop delay = L + ½ (averaging filter) + Δ (allpass), with Δ in [0.1, 1.1).
    let l = (period - 0.6).floor() as usize;
    let delta = period - 0.5 - l as f64;
    let c_ap = (1.0 - delta) / (1.0 + delta);
    let g = 10f64.powf(-3.0 / (t60 as f64 * freq as f64));

    let beta = (pluck_pos as f64).clamp(0.02, 0.98);
    let mut exc: Vec<f64> = (0..l)
        .map(|i| {
            let x = i as f64 / l as f64;
            let tri = if x < beta { x / beta } else { (1.0 - x) / (1.0 - beta) };
            tri + noise as f64 * (rng.uniform() as f64 * 2.0 - 1.0)
        })
        .collect();
    let mean = exc.iter().sum::<f64>() / l as f64;
    exc.iter_mut().for_each(|v| *v -= mean);
    let peak = exc.iter().fold(0f64, |a, v| a.max(v.abs())).max(1e-9);
    let mut line: Vec<f64> = exc.iter().map(|v| v / peak * amp as f64).collect();

    let mut out = Vec::with_capacity(n);
    let (mut idx, mut prev, mut ap_x1, mut ap_y1) = (0usize, 0f64, 0f64, 0f64);
    for _ in 0..n {
        let d = line[idx];
        let lp = g * 0.5 * (d + prev);
        prev = d;
        let ap = c_ap * lp + ap_x1 - c_ap * ap_y1;
        ap_x1 = lp;
        ap_y1 = ap;
        line[idx] = ap;
        out.push(d as f32);
        idx = (idx + 1) % l;
    }
    out
}

/// Strums `notes` low to high, `strum_ms` apart.
pub fn strum(notes: &[Pluck], sr: f32, strum_ms: f32, rng: &mut Pcg32) -> Vec<f32> {
    let mut out = Vec::new();
    for (k, p) in notes.iter().enumerate() {
        let offset = (k as f32 * strum_ms * 1e-3 * sr) as usize;
        let s = p.render(sr, rng);
        mix_into(&mut out, &s, offset, 1.0);
    }
    out
}
