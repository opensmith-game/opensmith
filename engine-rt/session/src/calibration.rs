//! Latency calibration: the player strums on clicks. The median delay between each click and
//! the strum the engine timed is the latency no device reported (Bluetooth, converters, the
//! player's own habit); the game then subtracts it from every attack.

use opensmith_audio::Track;

/// Strums farther than this from their click are ignored (s).
const MATCH: f64 = 0.25;
/// Fewer matched strums than this give no result.
pub const MIN_HITS: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationPlan {
    /// Song time of the first click (s).
    pub first: f64,
    pub period: f64,
    /// Clicks that only announce the beat.
    pub count_in: usize,
    /// Clicks the player strums on.
    pub clicks: usize,
}

impl Default for CalibrationPlan {
    fn default() -> Self {
        CalibrationPlan { first: 1.0, period: 0.6, count_in: 4, clicks: 12 }
    }
}

impl CalibrationPlan {
    /// Song time of click `k` (count-in included).
    pub fn click(&self, k: usize) -> f64 {
        self.first + k as f64 * self.period
    }

    /// When the last strum can no longer come.
    pub fn end(&self) -> f64 {
        self.click(self.count_in + self.clicks - 1) + MATCH + 0.2
    }

    /// The click track, stereo at `sample_rate`: high clicks count in, accents mark each bar.
    pub fn track(&self, sample_rate: u32) -> Track {
        let clicks: Vec<(f64, f64)> = (0..self.count_in + self.clicks)
            .map(|k| {
                let freq = if k < self.count_in {
                    1760.0
                } else if (k - self.count_in) % 4 == 0 {
                    1320.0
                } else {
                    880.0
                };
                (self.click(k), freq)
            })
            .collect();
        click_track(&clicks, self.end() + 0.5, sample_rate)
    }
}

/// A metronome on `beats` (song time, whether it starts a bar: a higher click), `seconds` long,
/// stereo at `sample_rate`: the lessons' accompaniment.
pub fn metronome(beats: &[(f64, bool)], seconds: f64, sample_rate: u32) -> Track {
    let clicks: Vec<(f64, f64)> = beats.iter().map(|&(time, bar)| (time, if bar { 1320.0 } else { 880.0 })).collect();
    click_track(&clicks, seconds, sample_rate)
}

/// Clicks (song time, pitch in Hz) of 50 ms, stereo, `seconds` long.
fn click_track(clicks: &[(f64, f64)], seconds: f64, sample_rate: u32) -> Track {
    let sr = sample_rate as f64;
    let frames = (seconds.max(0.0) * sr) as usize;
    let mut data = vec![0f32; frames * 2];
    for &(time, freq) in clicks {
        let start = (time * sr) as usize;
        for i in 0..(0.05 * sr) as usize {
            let t = i as f64 / sr;
            let v = (0.6 * (-t / 0.012).exp() * (std::f64::consts::TAU * freq * t).sin()) as f32;
            if let Some(frame) = data.get_mut(2 * (start + i)..2 * (start + i) + 2) {
                frame.fill(v);
            }
        }
    }
    Track::from_interleaved(&data, 2)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalibrationResult {
    /// Median delay of the strums after their clicks (s): the offset to compensate.
    pub offset: f64,
    /// Robust standard deviation of the delays (s): how steady the player was.
    pub spread: f64,
    pub hits: usize,
    pub total: usize,
}

#[derive(Clone, Debug)]
pub struct Calibration {
    pub plan: CalibrationPlan,
    errors: Vec<Option<f64>>,
}

impl Calibration {
    pub fn new(plan: CalibrationPlan) -> Calibration {
        Calibration { plan, errors: vec![None; plan.clicks] }
    }

    /// A strum at song time `t`: the measured click it answers and its delay, if it answers one
    /// that has none yet.
    pub fn strum(&mut self, t: f64) -> Option<(usize, f64)> {
        let first = self.plan.click(self.plan.count_in);
        let j = ((t - first) / self.plan.period).round();
        if j < 0.0 || j >= self.plan.clicks as f64 {
            return None;
        }
        let j = j as usize;
        let error = t - self.plan.click(self.plan.count_in + j);
        if error.abs() > MATCH || self.errors[j].is_some() {
            return None;
        }
        self.errors[j] = Some(error);
        Some((j, error))
    }

    /// Offset and spread need `MIN_HITS` strums; with fewer both are 0.
    pub fn result(&self) -> CalibrationResult {
        let mut delays: Vec<f64> = self.errors.iter().flatten().copied().collect();
        let (hits, total) = (delays.len(), self.plan.clicks);
        if hits < MIN_HITS {
            return CalibrationResult { offset: 0.0, spread: 0.0, hits, total };
        }
        let offset = median(&mut delays);
        let mut deviations: Vec<f64> = delays.iter().map(|d| (d - offset).abs()).collect();
        CalibrationResult { offset, spread: 1.4826 * median(&mut deviations), hits, total }
    }
}

fn median(x: &mut [f64]) -> f64 {
    x.sort_by(f64::total_cmp);
    let n = x.len();
    if n % 2 == 1 { x[n / 2] } else { 0.5 * (x[n / 2 - 1] + x[n / 2]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_delay_is_the_latency_to_compensate() {
        let plan = CalibrationPlan::default();
        let mut cal = Calibration::new(plan);
        assert_eq!(cal.strum(plan.click(1)), None, "count-in strums do not count");
        for j in 0..plan.clicks {
            let jitter = [0.004, -0.003, 0.0, 0.006, -0.005, 0.002][j % 6];
            let t = plan.click(plan.count_in + j) + 0.023 + jitter;
            assert_eq!(cal.strum(t).map(|h| h.0), Some(j));
            assert_eq!(cal.strum(t + 0.05), None, "one strum per click");
        }
        let r = cal.result();
        assert_eq!((r.hits, r.total), (12, 12));
        assert!((r.offset - 0.023).abs() < 0.002, "{r:?}");
        assert!(r.spread < 0.008, "{r:?}");
        assert_eq!(cal.strum(plan.click(plan.count_in) + 0.4), None, "too far from any click");
    }

    #[test]
    fn too_few_strums_give_no_offset() {
        let plan = CalibrationPlan::default();
        let mut cal = Calibration::new(plan);
        for j in 0..MIN_HITS - 1 {
            cal.strum(plan.click(plan.count_in + j) + 0.1);
        }
        assert_eq!(cal.result(), CalibrationResult { offset: 0.0, spread: 0.0, hits: MIN_HITS - 1, total: 12 });
    }

    #[test]
    fn the_click_track_clicks_on_time() {
        let plan = CalibrationPlan::default();
        let track = plan.track(48_000);
        let s = track.samples();
        let at = |t: f64| (t * 48_000.0) as usize * 2;
        assert!(s[at(plan.click(5))..at(plan.click(5)) + 200].iter().any(|v| v.abs() > 0.3));
        assert!(s[at(plan.click(5)) - 2000..at(plan.click(5))].iter().all(|v| *v == 0.0));
        assert!(track.frames() as f64 / 48_000.0 > plan.end());
    }

    #[test]
    fn the_metronome_clicks_on_each_beat_higher_on_the_bar() {
        let sr = 48_000;
        let track = metronome(&[(0.0, true), (0.5, false), (1.0, false)], 2.0, sr);
        assert_eq!(track.frames(), 2 * sr as usize);
        // Loud right after each beat, silent between them.
        let level = |from: f64, to: f64| {
            let (a, b) = ((from * sr as f64) as usize, (to * sr as f64) as usize);
            (a..b).map(|i| track.samples()[2 * i].abs()).fold(0.0f32, f32::max)
        };
        for beat in [0.0, 0.5, 1.0] {
            assert!(level(beat, beat + 0.01) > 0.3, "click at {beat}");
            assert!(level(beat + 0.1, beat + 0.4) == 0.0, "silence after {beat}");
        }
        assert_eq!(level(1.1, 2.0), 0.0);
        // The bar's click is higher: more zero crossings in its first 10 ms.
        let crossings = |at: f64| {
            let a = (at * sr as f64) as usize + 1;
            (a..a + 480).filter(|&i| (track.samples()[2 * i] > 0.0) != (track.samples()[2 * i + 2] > 0.0)).count()
        };
        assert!(crossings(0.0) > crossings(0.5));
    }
}
