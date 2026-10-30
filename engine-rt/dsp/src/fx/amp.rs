//! Amp models: a few voicings of the drive (clean, crunch, lead, metal) under the knobs a
//! guitarist expects — gain, bass, middle, treble, volume — plus echo and room.

use super::biquad::Biquad;
use super::chain::ChainParams;
use super::drive::DriveParams;
use super::gate::GateParams;
use super::space::{DelayParams, ReverbParams};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToneParams {
    pub bass_db: f32,
    pub mid_db: f32,
    pub treble_db: f32,
}

/// Bass (low shelf, 120 Hz), middle (peak, 650 Hz) and treble (high shelf, 2.8 kHz) after the
/// drive, as an amp's tone stack sits after its preamp.
pub struct ToneStack {
    params: ToneParams,
    bass: Biquad,
    mid: Biquad,
    treble: Biquad,
}

impl ToneStack {
    pub fn new(sr: f32, params: ToneParams) -> Self {
        let mut t = Self { params, bass: Biquad::IDENTITY, mid: Biquad::IDENTITY, treble: Biquad::IDENTITY };
        t.set_params(sr, params);
        t
    }

    pub fn set_params(&mut self, sr: f32, p: ToneParams) {
        self.params = p;
        self.bass = Biquad::low_shelf(sr, 120.0, 0.707, p.bass_db);
        self.mid = Biquad::peaking(sr, 650.0, 0.8, p.mid_db);
        self.treble = Biquad::high_shelf(sr, 2800.0, 0.707, p.treble_db);
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if self.params == ToneParams::default() {
            return;
        }
        for x in buf {
            *x = self.treble.process(self.mid.process(self.bass.process(*x)));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmpModel {
    /// Glassy, a touch of grit when dug into.
    Clean,
    /// Classic rock break-up.
    Crunch,
    /// Singing, mid-forward sustain.
    Lead,
    /// Tight, saturated, gated.
    Metal,
}

impl AmpModel {
    pub fn from_name(name: &str) -> Option<AmpModel> {
        match name.to_ascii_lowercase().as_str() {
            "clean" => Some(AmpModel::Clean),
            "crunch" => Some(AmpModel::Crunch),
            "lead" => Some(AmpModel::Lead),
            "metal" => Some(AmpModel::Metal),
            _ => None,
        }
    }
}

/// The knobs, each 0..1 (tone knobs at 0.5 are flat).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmpKnobs {
    pub gain: f32,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub volume: f32,
    pub delay: f32,
    pub reverb: f32,
    pub gate: bool,
}

impl Default for AmpKnobs {
    fn default() -> Self {
        Self { gain: 0.5, bass: 0.5, mid: 0.5, treble: 0.5, volume: 0.8, delay: 0.0, reverb: 0.0, gate: true }
    }
}

struct Voicing {
    /// Drive range swept by the gain knob.
    drive: (f32, f32),
    bias: f32,
    tight_hz: f32,
    mid_push_db: f32,
    brightness: f32,
    /// Level after the clipper, evening out the models' loudness.
    level_db: f32,
    gate_db: f32,
}

fn voicing(model: AmpModel) -> Voicing {
    match model {
        AmpModel::Clean => Voicing { drive: (0.0, 0.25), bias: 0.05, tight_hz: 60.0, mid_push_db: 0.0, brightness: 0.85, level_db: 0.0, gate_db: -66.0 },
        AmpModel::Crunch => Voicing { drive: (0.25, 0.55), bias: 0.25, tight_hz: 90.0, mid_push_db: 3.0, brightness: 0.6, level_db: -8.0, gate_db: -60.0 },
        AmpModel::Lead => Voicing { drive: (0.5, 0.8), bias: 0.2, tight_hz: 110.0, mid_push_db: 6.0, brightness: 0.5, level_db: -11.0, gate_db: -56.0 },
        AmpModel::Metal => Voicing { drive: (0.7, 1.0), bias: 0.1, tight_hz: 150.0, mid_push_db: 1.0, brightness: 0.6, level_db: -13.0, gate_db: -50.0 },
    }
}

impl ChainParams {
    /// The chain of `model` with its knobs at `k` (input gain and cabinet as in `self`).
    pub fn with_amp(self, model: AmpModel, k: AmpKnobs) -> ChainParams {
        let v = voicing(model);
        let knob = |x: f32| x.clamp(0.0, 1.0);
        let tone = |x: f32| (knob(x) - 0.5) * 24.0;
        ChainParams {
            gate: GateParams { enabled: k.gate, threshold_db: v.gate_db, ..self.gate },
            drive: DriveParams {
                enabled: true,
                drive: v.drive.0 + (v.drive.1 - v.drive.0) * knob(k.gain),
                tone: v.brightness,
                level_db: v.level_db,
                bias: v.bias,
                tight_hz: v.tight_hz,
                mid_db: v.mid_push_db,
            },
            tone: ToneParams { bass_db: tone(k.bass), mid_db: tone(k.mid), treble_db: tone(k.treble) },
            delay: DelayParams { mix: 0.45 * knob(k.delay), feedback: 0.25 + 0.3 * knob(k.delay), ..DelayParams::default() },
            reverb: ReverbParams { mix: 0.6 * knob(k.reverb), ..ReverbParams::default() },
            output_db: -24.0 + 30.0 * knob(k.volume),
            ..self
        }
    }
}
