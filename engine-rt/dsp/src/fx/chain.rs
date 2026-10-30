//! Mono guitar chain: input gain → noise gate → overdrive → tone stack → cabinet IR → delay →
//! reverb → output gain → safety soft limiter (output never reaches ±1.0, whatever the
//! settings). `ChainParams::with_amp` sets it up as one of the amp models.

use super::amp::{ToneParams, ToneStack};
use super::cabinet::{CabinetIr, Convolver};
use super::drive::{DriveParams, Overdrive};
use super::gate::{GateParams, NoiseGate};
use super::space::{Delay, DelayParams, Reverb, ReverbParams};
use crate::db_to_lin;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChainParams {
    pub input_gain_db: f32,
    pub gate: GateParams,
    pub drive: DriveParams,
    pub tone: ToneParams,
    pub cab_enabled: bool,
    pub delay: DelayParams,
    pub reverb: ReverbParams,
    pub output_db: f32,
}

impl Default for ChainParams {
    fn default() -> Self {
        Self {
            input_gain_db: 0.0,
            gate: GateParams::default(),
            drive: DriveParams::default(),
            tone: ToneParams::default(),
            cab_enabled: true,
            delay: DelayParams::default(),
            reverb: ReverbParams::default(),
            output_db: 0.0,
        }
    }
}

pub struct GuitarChain {
    sr: f32,
    params: ChainParams,
    gate: NoiseGate,
    drive: Overdrive,
    tone: ToneStack,
    cab: Convolver,
    delay: Delay,
    reverb: Reverb,
    in_gain: f32,
    out_gain: f32,
}

/// Soft knee above 0.9 with a 0.99 ceiling (−0.09 dBFS): identity below the knee, never
/// reaches full scale (tanh saturates to exactly 1.0 in f32, hence a ceiling below 1).
#[inline]
pub fn safety_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.9;
    const CEILING: f32 = 0.99;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (CEILING - KNEE) * ((a - KNEE) / (CEILING - KNEE)).tanh())
    }
}

impl GuitarChain {
    /// `ir = None` uses the built-in 4×12 cabinet.
    pub fn new(sr: f32, params: ChainParams, ir: Option<&CabinetIr>) -> Self {
        let builtin;
        let ir = match ir {
            Some(ir) => ir,
            None => {
                builtin = CabinetIr::builtin_4x12(sr);
                &builtin
            }
        };
        Self {
            sr,
            params,
            gate: NoiseGate::new(sr, params.gate),
            drive: Overdrive::new(sr, params.drive),
            tone: ToneStack::new(sr, params.tone),
            cab: Convolver::new(&ir.samples, 64),
            delay: Delay::new(sr, params.delay),
            reverb: Reverb::new(sr, params.reverb),
            in_gain: db_to_lin(params.input_gain_db),
            out_gain: db_to_lin(params.output_db),
        }
    }

    pub fn params(&self) -> &ChainParams {
        &self.params
    }

    /// Allocation-free parameter update (coefficients are recomputed in place).
    pub fn set_params(&mut self, p: ChainParams) {
        self.params = p;
        self.gate.set_params(self.sr, p.gate);
        self.drive.set_params(self.sr, p.drive);
        self.tone.set_params(self.sr, p.tone);
        self.delay.set_params(p.delay);
        self.reverb.set_params(p.reverb);
        self.in_gain = db_to_lin(p.input_gain_db);
        self.out_gain = db_to_lin(p.output_db);
    }

    pub fn gate_open(&self) -> bool {
        self.gate.is_open()
    }

    /// Processes a mono block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.in_gain != 1.0 {
            buf.iter_mut().for_each(|v| *v *= self.in_gain);
        }
        self.gate.process(buf);
        self.drive.process(buf);
        self.tone.process(buf);
        if self.params.cab_enabled {
            self.cab.process(buf);
        }
        self.delay.process(buf);
        self.reverb.process(buf);
        for v in buf.iter_mut() {
            *v = safety_limit(*v * self.out_gain);
        }
    }
}
