//! RBJ "Audio EQ Cookbook" biquads, transposed direct form II, plus a DC blocker.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    pub const IDENTITY: Biquad = Biquad { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 };

    fn from_coefs(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: (b0 / a0) as f32,
            b1: (b1 / a0) as f32,
            b2: (b2 / a0) as f32,
            a1: (a1 / a0) as f32,
            a2: (a2 / a0) as f32,
            z1: 0.0,
            z2: 0.0,
        }
    }

    fn omega(sr: f32, f: f32, q: f32) -> (f64, f64, f64) {
        let w0 = 2.0 * PI * (f as f64).min(0.49 * sr as f64) / sr as f64;
        let (s, c) = w0.sin_cos();
        (c, s / (2.0 * q as f64), s)
    }

    pub fn lowpass(sr: f32, f: f32, q: f32) -> Self {
        let (c, alpha, _) = Self::omega(sr, f, q);
        Self::from_coefs((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    pub fn highpass(sr: f32, f: f32, q: f32) -> Self {
        let (c, alpha, _) = Self::omega(sr, f, q);
        Self::from_coefs((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    pub fn peaking(sr: f32, f: f32, q: f32, gain_db: f32) -> Self {
        let (c, alpha, _) = Self::omega(sr, f, q);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        Self::from_coefs(1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a)
    }

    pub fn low_shelf(sr: f32, f: f32, q: f32, gain_db: f32) -> Self {
        let (c, alpha, _) = Self::omega(sr, f, q);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_coefs(
            a * ((a + 1.0) - (a - 1.0) * c + sq),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - sq),
            (a + 1.0) + (a - 1.0) * c + sq,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - sq,
        )
    }

    pub fn high_shelf(sr: f32, f: f32, q: f32, gain_db: f32) -> Self {
        let (c, alpha, _) = Self::omega(sr, f, q);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let sq = 2.0 * a.sqrt() * alpha;
        Self::from_coefs(
            a * ((a + 1.0) + (a - 1.0) * c + sq),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - sq),
            (a + 1.0) - (a - 1.0) * c + sq,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - sq,
        )
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Magnitude response in dB at `f` Hz.
    pub fn response_db(&self, sr: f32, f: f32) -> f32 {
        let w = 2.0 * PI * f as f64 / sr as f64;
        let (c1, s1) = (w.cos(), -w.sin());
        let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
        let num = (self.b0 as f64 + self.b1 as f64 * c1 + self.b2 as f64 * c2, self.b1 as f64 * s1 + self.b2 as f64 * s2);
        let den = (1.0 + self.a1 as f64 * c1 + self.a2 as f64 * c2, self.a1 as f64 * s1 + self.a2 as f64 * s2);
        let mag2 = (num.0 * num.0 + num.1 * num.1) / (den.0 * den.0 + den.1 * den.1);
        (10.0 * mag2.max(1e-30).log10()) as f32
    }
}

/// First-order DC blocker: y[n] = x[n] − x[n−1] + R·y[n−1].
#[derive(Clone, Copy, Debug)]
pub struct DcBlocker {
    r: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    pub fn new(sr: f32, cutoff_hz: f32) -> Self {
        Self { r: 1.0 - std::f32::consts::TAU * cutoff_hz / sr, x1: 0.0, y1: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookbook_responses() {
        let sr = 48_000.0;
        let lp = Biquad::lowpass(sr, 1000.0, std::f32::consts::FRAC_1_SQRT_2);
        assert!((lp.response_db(sr, 1000.0) + 3.01).abs() < 0.05);
        assert!(lp.response_db(sr, 50.0).abs() < 0.01);
        assert!(lp.response_db(sr, 10_000.0) < -38.0);
        let hp = Biquad::highpass(sr, 100.0, std::f32::consts::FRAC_1_SQRT_2);
        assert!((hp.response_db(sr, 100.0) + 3.01).abs() < 0.05);
        let pk = Biquad::peaking(sr, 800.0, 1.0, 6.0);
        assert!((pk.response_db(sr, 800.0) - 6.0).abs() < 0.01);
        let hs = Biquad::high_shelf(sr, 3000.0, 0.707, -6.0);
        assert!((hs.response_db(sr, 20_000.0) + 6.0).abs() < 0.3);
    }

    #[test]
    fn dc_blocker_removes_offset() {
        let mut dc = DcBlocker::new(48_000.0, 10.0);
        let mut last = 0.0;
        for i in 0..48_000 {
            last = dc.process(0.5 + 0.1 * (i as f32 * 0.05).sin());
        }
        assert!(last.abs() < 0.11);
    }
}
