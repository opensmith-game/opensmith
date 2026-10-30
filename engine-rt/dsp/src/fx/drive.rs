//! Overdrive: tightening high-pass + mid push → gain → asymmetric tanh clipper with first-order
//! antiderivative anti-aliasing (ADAA) → DC blocker → tone low-pass → level.
//!
//! ADAA (Parker, Zavalishin & Le Bivic, DAFx 2016) replaces f(x[n]) by the mean of f over the
//! segment [x[n−1], x[n]], i.e. (F(x[n]) − F(x[n−1])) / (x[n] − x[n−1]) with F' = f, which
//! suppresses aliasing of the clipper's harmonics without oversampling (½ sample of delay).

use super::biquad::{Biquad, DcBlocker};
use crate::db_to_lin;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DriveParams {
    pub enabled: bool,
    /// 0..1 → 0..48 dB of gain into the clipper.
    pub drive: f32,
    /// 0..1 → tone low-pass from 1.2 kHz to 9.6 kHz.
    pub tone: f32,
    pub level_db: f32,
    /// Clipper bias: asymmetry → even harmonics.
    pub bias: f32,
    /// Pre-clip high-pass (tightens the low end).
    pub tight_hz: f32,
    /// Pre-clip mid push at 800 Hz.
    pub mid_db: f32,
}

impl Default for DriveParams {
    fn default() -> Self {
        Self { enabled: true, drive: 0.5, tone: 0.5, level_db: -6.0, bias: 0.2, tight_hz: 110.0, mid_db: 6.0 }
    }
}

/// Antiderivative-anti-aliased asymmetric tanh: f(u) = tanh(u + b) − tanh(b).
#[derive(Clone, Copy, Debug)]
pub struct AdaaTanh {
    bias: f64,
    tb: f64,
    x1: f64,
    f1: f64,
}

#[inline]
fn log_cosh(v: f64) -> f64 {
    let a = v.abs();
    a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2
}

impl AdaaTanh {
    pub fn new(bias: f32) -> Self {
        let bias = bias as f64;
        Self { bias, tb: bias.tanh(), x1: 0.0, f1: log_cosh(bias) }
    }

    #[inline]
    fn antiderivative(&self, u: f64) -> f64 {
        log_cosh(u + self.bias) - u * self.tb
    }

    #[inline]
    pub fn process(&mut self, u: f64) -> f64 {
        let fu = self.antiderivative(u);
        let d = u - self.x1;
        let y = if d.abs() > 1e-6 {
            (fu - self.f1) / d
        } else {
            (0.5 * (u + self.x1) + self.bias).tanh() - self.tb
        };
        self.x1 = u;
        self.f1 = fu;
        y
    }

    /// The memoryless curve (for comparisons).
    pub fn naive(&self, u: f64) -> f64 {
        (u + self.bias).tanh() - self.tb
    }
}

pub struct Overdrive {
    params: DriveParams,
    pre_hp: Biquad,
    mid: Biquad,
    clip: AdaaTanh,
    dc: DcBlocker,
    tone: Biquad,
    gain: f32,
    level: f32,
}

impl Overdrive {
    pub fn new(sr: f32, params: DriveParams) -> Self {
        let mut d = Self {
            params,
            pre_hp: Biquad::IDENTITY,
            mid: Biquad::IDENTITY,
            clip: AdaaTanh::new(params.bias),
            dc: DcBlocker::new(sr, 8.0),
            tone: Biquad::IDENTITY,
            gain: 1.0,
            level: 1.0,
        };
        d.set_params(sr, params);
        d
    }

    pub fn set_params(&mut self, sr: f32, p: DriveParams) {
        self.params = p;
        self.pre_hp = Biquad::highpass(sr, p.tight_hz, 0.707);
        self.mid = Biquad::peaking(sr, 800.0, 0.8, p.mid_db);
        if self.clip.bias != p.bias as f64 {
            self.clip = AdaaTanh::new(p.bias);
        }
        self.tone = Biquad::lowpass(sr, 1200.0 * 8f32.powf(p.tone.clamp(0.0, 1.0)), 0.707);
        self.gain = db_to_lin(48.0 * p.drive.clamp(0.0, 1.0));
        self.level = db_to_lin(p.level_db);
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if !self.params.enabled {
            return;
        }
        for x in buf {
            let s = self.mid.process(self.pre_hp.process(*x));
            let y = self.clip.process((s * self.gain) as f64) as f32;
            let y = self.tone.process(self.dc.process(y));
            *x = y * self.level;
        }
    }
}
