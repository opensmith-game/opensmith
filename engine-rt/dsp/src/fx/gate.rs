//! Noise gate with hysteresis, hold and smoothed gain (no clicks).

use super::time_coef;
use crate::db_to_lin;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GateParams {
    pub enabled: bool,
    /// Opening level of the mean-|x| envelope (dBFS).
    pub threshold_db: f32,
    /// The gate closes `hysteresis_db` below the opening level.
    pub hysteresis_db: f32,
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub release_ms: f32,
    /// Attenuation when closed (dB, negative).
    pub range_db: f32,
}

impl Default for GateParams {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_db: -58.0,
            hysteresis_db: 6.0,
            attack_ms: 0.5,
            hold_ms: 60.0,
            release_ms: 120.0,
            range_db: -80.0,
        }
    }
}

pub struct NoiseGate {
    params: GateParams,
    env: f32,
    env_up: f32,
    env_down: f32,
    open_th: f32,
    close_th: f32,
    hold: u32,
    hold_left: u32,
    open: bool,
    gain: f32,
    floor: f32,
    att: f32,
    rel: f32,
}

impl NoiseGate {
    pub fn new(sr: f32, params: GateParams) -> Self {
        let mut g = Self {
            params,
            env: 0.0,
            env_up: time_coef(1.0, sr),
            env_down: time_coef(20.0, sr),
            open_th: 0.0,
            close_th: 0.0,
            hold: 0,
            hold_left: 0,
            open: false,
            gain: 0.0,
            floor: 0.0,
            att: 0.0,
            rel: 0.0,
        };
        g.set_params(sr, params);
        g.gain = g.floor;
        g
    }

    pub fn set_params(&mut self, sr: f32, p: GateParams) {
        self.params = p;
        self.open_th = db_to_lin(p.threshold_db);
        self.close_th = db_to_lin(p.threshold_db - p.hysteresis_db);
        self.hold = (p.hold_ms * 1e-3 * sr) as u32;
        self.floor = db_to_lin(p.range_db);
        self.att = time_coef(p.attack_ms, sr);
        self.rel = time_coef(p.release_ms, sr);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Current gain applied (linear).
    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if !self.params.enabled {
            return;
        }
        for x in buf {
            let a = x.abs();
            let c = if a > self.env { self.env_up } else { self.env_down };
            self.env += c * (a - self.env);
            if self.env >= self.open_th {
                self.open = true;
                self.hold_left = self.hold;
            } else if self.env < self.close_th {
                if self.hold_left > 0 {
                    self.hold_left -= 1;
                } else {
                    self.open = false;
                }
            }
            let target = if self.open { 1.0 } else { self.floor };
            let c = if target > self.gain { self.att } else { self.rel };
            self.gain += c * (target - self.gain);
            *x *= self.gain;
        }
    }
}
