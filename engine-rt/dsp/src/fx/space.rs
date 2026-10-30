//! Delay and reverb after the cabinet. Every buffer is sized at construction for the longest
//! setting, so changing parameters never allocates.

/// Longest echo (ms).
pub const MAX_DELAY_MS: f32 = 1500.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DelayParams {
    /// Echo level, 0 = off.
    pub mix: f32,
    pub time_ms: f32,
    /// Share of each echo fed back into the next one (< 1).
    pub feedback: f32,
}

impl Default for DelayParams {
    fn default() -> Self {
        Self { mix: 0.0, time_ms: 380.0, feedback: 0.35 }
    }
}

/// Mono echo whose repeats darken (a low-pass in the feedback path, as on analog delays).
pub struct Delay {
    sr: f32,
    params: DelayParams,
    line: Vec<f32>,
    at: usize,
    delay: usize,
    damp: f32,
    lp: f32,
}

impl Delay {
    pub fn new(sr: f32, params: DelayParams) -> Self {
        let mut d = Self {
            sr,
            params,
            line: vec![0.0; (MAX_DELAY_MS * 1e-3 * sr) as usize + 1],
            at: 0,
            delay: 1,
            damp: 0.0,
            lp: 0.0,
        };
        d.set_params(params);
        d
    }

    pub fn set_params(&mut self, p: DelayParams) {
        self.params = p;
        let samples = (p.time_ms.clamp(1.0, MAX_DELAY_MS) * 1e-3 * self.sr) as usize;
        self.delay = samples.clamp(1, self.line.len() - 1);
        self.damp = (-2.0 * std::f32::consts::PI * 4000.0 / self.sr).exp();
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if self.params.mix <= 0.0 {
            return;
        }
        let n = self.line.len();
        let feedback = self.params.feedback.clamp(0.0, 0.95);
        for x in buf {
            let echo = self.line[(self.at + n - self.delay) % n];
            self.lp = echo + self.damp * (self.lp - echo);
            self.line[self.at] = *x + self.lp * feedback;
            self.at = (self.at + 1) % n;
            *x += self.params.mix * echo;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReverbParams {
    /// Room level, 0 = off.
    pub mix: f32,
    /// 0..1: small room to hall.
    pub size: f32,
    /// 0..1: how fast the highs die away.
    pub damping: f32,
}

impl Default for ReverbParams {
    fn default() -> Self {
        Self { mix: 0.0, size: 0.7, damping: 0.4 }
    }
}

struct Comb {
    buf: Vec<f32>,
    at: usize,
    store: f32,
}

struct Allpass {
    buf: Vec<f32>,
    at: usize,
}

/// Freeverb (Jezar at Dreampoint): eight damped feedback combs in parallel, four all-passes in
/// series, delay lengths scaled from their 44.1 kHz values.
pub struct Reverb {
    params: ReverbParams,
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    feedback: f32,
    damp: f32,
}

const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];

impl Reverb {
    pub fn new(sr: f32, params: ReverbParams) -> Self {
        let scale = sr / 44_100.0;
        let len = |n: usize| ((n as f32 * scale) as usize).max(1);
        let mut r = Self {
            params,
            combs: COMBS.iter().map(|&n| Comb { buf: vec![0.0; len(n)], at: 0, store: 0.0 }).collect(),
            allpasses: ALLPASSES.iter().map(|&n| Allpass { buf: vec![0.0; len(n)], at: 0 }).collect(),
            feedback: 0.0,
            damp: 0.0,
        };
        r.set_params(params);
        r
    }

    pub fn set_params(&mut self, p: ReverbParams) {
        self.params = p;
        self.feedback = 0.7 + 0.28 * p.size.clamp(0.0, 1.0);
        self.damp = 0.4 * p.damping.clamp(0.0, 1.0);
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        if self.params.mix <= 0.0 {
            return;
        }
        for x in buf {
            let input = *x * 0.015;
            let mut wet = 0.0;
            for c in &mut self.combs {
                let out = c.buf[c.at];
                c.store = out * (1.0 - self.damp) + c.store * self.damp;
                c.buf[c.at] = input + c.store * self.feedback;
                c.at = (c.at + 1) % c.buf.len();
                wet += out;
            }
            for a in &mut self.allpasses {
                let held = a.buf[a.at];
                a.buf[a.at] = wet + held * 0.5;
                a.at = (a.at + 1) % a.buf.len();
                wet = held - wet;
            }
            *x += self.params.mix * 3.0 * wet;
        }
    }
}
