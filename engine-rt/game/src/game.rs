//! Game rules: judging the player against the chart, score and streak, dynamic difficulty.
//!
//! Times are song seconds (the backing track's clock). Notes are judged on their *attack*
//! timestamp from the tracker, known within a millisecond, even though the pitch is confirmed
//! up to ~18 ms later on the low E: a late confirmation never makes a hit late. Chords are
//! judged by the chord verifier, which the engine runs on the audio of the chords listed by
//! `chords_ready`.
//!
//! Dynamic difficulty: every phrase name has its own level. When a phrase
//! iteration is over, a good run (≥ `level_up` accuracy) raises that phrase's level, a poor one
//! (< `level_down`) lowers it. The level of an iteration is fixed when it becomes visible,
//! `lookahead` seconds before it starts, so notes never change under the player's eyes.

use std::collections::HashMap;

use crate::chart::Chart;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameConfig {
    /// A note counts when played within ± this many seconds of its time.
    pub hit_window: f64,
    /// Tighter window for a "perfect".
    pub perfect_window: f64,
    /// Seconds subtracted from player timestamps (input + output latency calibration).
    pub input_offset: f64,
    /// Phrases become visible, and their level is fixed, this long before they start.
    pub lookahead: f64,
    pub dynamic_difficulty: bool,
    /// Level of every phrase at the start (and the fixed level without dynamic difficulty).
    pub start_level: u32,
    pub level_up: f64,
    pub level_down: f64,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            hit_window: 0.10,
            perfect_window: 0.04,
            input_offset: 0.0,
            lookahead: 3.0,
            dynamic_difficulty: true,
            start_level: 0,
            level_up: 0.85,
            level_down: 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Note { string: u8, fret: u8, midi: i32 },
    Chord { template: usize, midis: Vec<i32> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Hit,
    Missed,
}

/// A note or chord the player has to play.
#[derive(Clone, Debug)]
pub struct Expected {
    /// Index in `Chart::notes` or `Chart::chords`, by target kind.
    pub event: usize,
    pub time: f64,
    pub sustain: f64,
    pub target: Target,
    /// Phrase iteration (index in `Chart::phrases`).
    pub iteration: usize,
    pub status: Status,
    /// Chords only: the verifier already looked at this strum.
    pub checked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Judgement {
    Hit { index: usize, time: f64, error: f64, perfect: bool },
    Miss { index: usize, time: f64 },
    PhraseDone { iteration: usize, name: String, accuracy: f64, level: u32, next_level: u32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Score {
    pub hits: u32,
    pub perfects: u32,
    pub misses: u32,
    /// Notes played that the chart did not ask for (not penalised).
    pub extras: u32,
    pub streak: u32,
    pub best_streak: u32,
    pub points: u64,
}

impl Score {
    pub fn accuracy(&self) -> f64 {
        let judged = self.hits + self.misses;
        if judged == 0 { 0.0 } else { self.hits as f64 / judged as f64 }
    }

    /// ×1 to ×4, one step per 10 notes in a row.
    pub fn multiplier(&self) -> u64 {
        (1 + self.streak as u64 / 10).min(4)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Iteration {
    level: u32,
    hits: u32,
    total: u32,
    activated: bool,
    done: bool,
}

pub struct Game {
    chart: Chart,
    cfg: GameConfig,
    levels: HashMap<String, u32>,
    iterations: Vec<Iteration>,
    next_iteration: usize,
    expected: Vec<Expected>,
    score: Score,
    /// Notes before this song time are not judged (seek, retry from a phrase).
    start_at: f64,
    /// Only notes in this span are shown and judged (Loop Trainer); kept across restarts.
    range: (f64, f64),
}

/// Extra time after a chord's hit window for its verification to come back.
const CHORD_GRACE: f64 = 0.1;
/// Extra time after a note's hit window for the tracker to confirm its pitch: a note attacked
/// late in the window is reported a few tens of milliseconds later (its attack time is judged).
const NOTE_GRACE: f64 = 0.06;

impl Game {
    pub fn new(chart: Chart, cfg: GameConfig) -> Game {
        let iterations = vec![Iteration::default(); chart.phrases.len()];
        Game {
            chart,
            cfg,
            levels: HashMap::new(),
            iterations,
            next_iteration: 0,
            expected: Vec::new(),
            score: Score::default(),
            start_at: f64::NEG_INFINITY,
            range: (f64::NEG_INFINITY, f64::INFINITY),
        }
    }

    /// Starts over at song time `t` (seek, retry): notes before `t` are not judged, score and
    /// streak start from zero, the levels learnt for each phrase are kept.
    pub fn restart_at(&mut self, t: f64) {
        self.expected.clear();
        self.score = Score::default();
        self.iterations = vec![Iteration::default(); self.chart.phrases.len()];
        self.next_iteration = 0;
        self.start_at = t;
        while self.next_iteration < self.chart.phrases.len() && self.chart.phrases[self.next_iteration].end <= t {
            self.iterations[self.next_iteration] = Iteration { activated: true, done: true, ..Iteration::default() };
            self.next_iteration += 1;
        }
    }

    pub fn chart(&self) -> &Chart {
        &self.chart
    }

    pub fn score(&self) -> Score {
        self.score
    }

    /// Current level of a phrase (dynamic difficulty).
    pub fn level(&self, phrase: &str) -> u32 {
        let start = self.cfg.start_level.min(self.chart.arrangement.max_difficulty);
        if self.cfg.dynamic_difficulty { *self.levels.get(phrase).unwrap_or(&start) } else { start }
    }

    /// Level at which a phrase iteration is played (fixed when it became visible).
    pub fn iteration_level(&self, iteration: usize) -> Option<u32> {
        self.iterations.get(iteration).filter(|i| i.activated).map(|i| i.level)
    }

    /// Everything made visible so far, in time order per phrase.
    pub fn expected(&self) -> &[Expected] {
        &self.expected
    }

    /// Makes every phrase visible now (previews, tests, offline tools).
    pub fn activate_all(&mut self) {
        self.activate(f64::INFINITY);
    }

    /// Levels the phrases start at (by name), learnt in earlier sessions; capped at the
    /// chart's top level.
    pub fn set_levels(&mut self, levels: HashMap<String, u32>) {
        let top = self.chart.arrangement.max_difficulty;
        self.levels = levels.into_iter().map(|(name, level)| (name, level.min(top))).collect();
    }

    /// Difficulty changed during the song (pause menu): dynamic difficulty from the levels
    /// learnt, or every phrase at `start_level`. The phrases already visible keep their level,
    /// the next ones take the new one.
    pub fn set_difficulty(&mut self, dynamic: bool, start_level: u32) {
        self.cfg.dynamic_difficulty = dynamic;
        self.cfg.start_level = start_level;
    }

    /// Loop Trainer: only the notes from `from` (included) to `to` are shown and judged
    /// (phrases already visible keep theirs: restart to apply it); infinite bounds for the
    /// whole song.
    pub fn set_range(&mut self, from: f64, to: f64) {
        self.range = (from, to);
    }

    /// Latency compensation: seconds subtracted from attack times (calibration result).
    pub fn set_input_offset(&mut self, seconds: f64) {
        self.cfg.input_offset = seconds;
    }

    /// Makes visible what the clock at `now` reveals (the other calls do it too); lets a caller
    /// announce new notes before judging anything.
    pub fn activate_until(&mut self, now: f64) {
        self.activate(now - self.cfg.input_offset);
    }

    fn activate(&mut self, now: f64) {
        while self.next_iteration < self.chart.phrases.len()
            && self.chart.phrases[self.next_iteration].start <= now + self.cfg.lookahead
        {
            let k = self.next_iteration;
            let phrase = &self.chart.phrases[k];
            let level = self.level(&phrase.name);
            let (start, end) = (phrase.start, phrase.end);
            let (from, to) = (self.start_at.max(self.range.0), self.range.1);
            let shown = |levels: [u32; 2], t: f64| {
                levels[0] <= level && level <= levels[1] && start <= t && t < end && t >= from && t < to
            };
            let mut new: Vec<Expected> = Vec::new();
            for (event, n) in self.chart.notes.iter().enumerate() {
                if shown(n.levels, n.time) {
                    let midi = self.chart.pitch(n.string, n.fret);
                    new.push(Expected {
                        event,
                        time: n.time,
                        sustain: n.sustain,
                        target: Target::Note { string: n.string, fret: n.fret, midi },
                        iteration: k,
                        status: Status::Pending,
                        checked: false,
                    });
                }
            }
            for (event, c) in self.chart.chords.iter().enumerate() {
                if shown(c.levels, c.time) {
                    new.push(Expected {
                        event,
                        time: c.time,
                        sustain: c.sustain,
                        target: Target::Chord { template: c.template, midis: self.chart.chord_pitches(c.template) },
                        iteration: k,
                        status: Status::Pending,
                        checked: false,
                    });
                }
            }
            new.sort_by(|a, b| a.time.total_cmp(&b.time));
            self.iterations[k] = Iteration { level, total: new.len() as u32, activated: true, ..Iteration::default() };
            self.expected.extend(new);
            self.next_iteration += 1;
        }
    }

    fn hit(&mut self, index: usize, error: f64) -> Judgement {
        let perfect = error.abs() <= self.cfg.perfect_window;
        let e = &mut self.expected[index];
        e.status = Status::Hit;
        self.iterations[e.iteration].hits += 1;
        let s = &mut self.score;
        s.hits += 1;
        s.perfects += perfect as u32;
        s.streak += 1;
        s.best_streak = s.best_streak.max(s.streak);
        s.points += (100 + 50 * perfect as u64) * s.multiplier();
        Judgement::Hit { index, time: e.time, error, perfect }
    }

    /// The tracker heard a note: attack at `time` (song seconds), pitch `midi`.
    pub fn on_note(&mut self, time: f64, midi: i32) -> Option<Judgement> {
        let t = time - self.cfg.input_offset;
        self.activate(t);
        let w = self.cfg.hit_window;
        let best = self
            .expected
            .iter()
            .enumerate()
            .filter(|(_, e)| e.status == Status::Pending && (e.time - t).abs() <= w)
            .filter(|(_, e)| matches!(e.target, Target::Note { midi: m, .. } if m == midi))
            .min_by(|a, b| (a.1.time - t).abs().total_cmp(&(b.1.time - t).abs()))
            .map(|(i, e)| (i, t - e.time));
        if let Some((index, error)) = best {
            return Some(self.hit(index, error));
        }
        let in_chord = self.expected.iter().any(|e| {
            (e.time - t).abs() <= w + CHORD_GRACE && matches!(&e.target, Target::Chord { midis, .. } if midis.contains(&midi))
        });
        if !in_chord {
            self.score.extras += 1;
        }
        None
    }

    /// The autopilot (demo mode): every pending note and chord whose time has come is played,
    /// right on time.
    pub fn autoplay(&mut self, now: f64) -> Vec<Judgement> {
        let t = now - self.cfg.input_offset;
        self.activate(t);
        let due: Vec<usize> =
            self.expected.iter().enumerate().filter(|(_, e)| e.status == Status::Pending && e.time <= t).map(|(i, _)| i).collect();
        due.into_iter().map(|i| self.hit(i, 0.0)).collect()
    }

    /// Pending chords whose strum is at least `analysis` seconds old and not verified yet:
    /// (index, time, pitches). Verify the audio from `time` and report with `on_chord`.
    pub fn chords_ready(&mut self, now: f64, analysis: f64) -> Vec<(usize, f64, Vec<i32>)> {
        let t = now - self.cfg.input_offset;
        self.activate(t);
        self.expected
            .iter()
            .enumerate()
            .filter(|(_, e)| e.status == Status::Pending && !e.checked && e.time + analysis <= t)
            .filter_map(|(i, e)| match &e.target {
                Target::Chord { midis, .. } => Some((i, e.time, midis.clone())),
                Target::Note { .. } => None,
            })
            .collect()
    }

    /// Result of the chord verifier for expected chord `index`; `attack` is the strum time
    /// if the tracker saw it.
    pub fn on_chord(&mut self, index: usize, accepted: bool, attack: Option<f64>) -> Option<Judgement> {
        let e = self.expected.get_mut(index)?;
        if e.status != Status::Pending || !matches!(e.target, Target::Chord { .. }) {
            return None;
        }
        e.checked = true;
        if !accepted {
            return None;
        }
        let error = attack.map_or(0.0, |a| a - self.cfg.input_offset - e.time);
        if error.abs() > self.cfg.hit_window {
            return None;
        }
        Some(self.hit(index, error))
    }

    /// Moves the clock to `now`: unplayed notes become misses, finished phrase iterations
    /// update dynamic difficulty.
    pub fn advance(&mut self, now: f64) -> Vec<Judgement> {
        let t = now - self.cfg.input_offset;
        self.activate(t);
        let mut out = Vec::new();
        let w = self.cfg.hit_window;
        for index in 0..self.expected.len() {
            let e = &self.expected[index];
            let grace = if matches!(e.target, Target::Chord { .. }) { CHORD_GRACE } else { NOTE_GRACE };
            if e.status == Status::Pending && e.time + w + grace < t {
                let time = e.time;
                self.expected[index].status = Status::Missed;
                self.score.misses += 1;
                self.score.streak = 0;
                out.push(Judgement::Miss { index, time });
            }
        }
        for k in 0..self.iterations.len() {
            let it = self.iterations[k];
            let phrase = &self.chart.phrases[k];
            if !it.activated || it.done || phrase.end + w + CHORD_GRACE >= t {
                continue;
            }
            self.iterations[k].done = true;
            if it.total == 0 {
                continue;
            }
            let accuracy = it.hits as f64 / it.total as f64;
            let mut next = it.level;
            if self.cfg.dynamic_difficulty {
                let top = self.chart.arrangement.max_difficulty;
                if accuracy >= self.cfg.level_up && it.level < top {
                    next = it.level + 1;
                } else if accuracy < self.cfg.level_down && it.level > 0 {
                    next = it.level - 1;
                }
                self.levels.insert(phrase.name.clone(), next);
            }
            out.push(Judgement::PhraseDone { iteration: k, name: phrase.name.clone(), accuracy, level: it.level, next_level: next });
        }
        out
    }
}
