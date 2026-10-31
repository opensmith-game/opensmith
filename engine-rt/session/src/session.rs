use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use opensmith_audio::{Monitor, PlayerClock, PlayerCommand, StreamInfo, Track, MAX_RATE, MIN_RATE};
use opensmith_dsp::{ChainParams, ChordConfig, ChordVerifier, TrackerEvent, TunerAnalyzer, TunerConfig};
use opensmith_game::{Chart, Game, GameConfig, Score};

use crate::calibration::{Calibration, CalibrationPlan};
use crate::events::{UiEvent, UiExpected};
use serde::Deserialize;
use crate::history::TapHistory;

/// Raw input reaches the session in tap blocks: allow for one block of delay (s).
const TAP_DELAY: f64 = 0.006;
/// Attacks older than this are forgotten (s).
const ATTACK_MEMORY: f64 = 2.0;
/// Step between the windows searched for a chord whose strum the tracker did not notice (s).
const SCAN_STEP: f64 = 0.015;
/// Tuner mode sends the pitch at most this often (s).
const PITCH_EVERY: f64 = 0.02;

/// Where a session gets its audio: the CoreAudio engine, or a simulation in tests.
pub trait AudioBackend {
    fn monitor(&mut self) -> &mut Monitor;
    /// Host time now (ns), the clock of the audio callback timestamps.
    fn now_ns(&self) -> u64;
    fn info(&self) -> Option<StreamInfo> {
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SessionConfig {
    pub game: GameConfig,
    /// Audio after a strum that the chord verifier looks at (s).
    pub chord_window: f64,
    /// Concert pitch.
    pub a4: f32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig { game: GameConfig { lookahead: 3.4, ..GameConfig::default() }, chord_window: 0.09, a4: 440.0 }
    }
}

/// Loop Trainer: the music loops over `start..end` (song seconds) and the notes from `from`
/// to `to` are judged again on every pass. `start` comes a little before `from` to get ready,
/// `end` long enough after `to` for the last notes to be judged before the loop goes back.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
pub struct RiffLoop {
    pub start: f64,
    pub end: f64,
    pub from: f64,
    pub to: f64,
}

/// One song being played: owns the game, drives the player, turns tracker events into
/// judgements. `update` is called every few milliseconds; `take_events` hands the UI its news.
pub struct Session<B: AudioBackend> {
    backend: B,
    cfg: SessionConfig,
    sample_rate: f64,
    game: Option<Game>,
    /// Chart time + offset = track time.
    offset: f64,
    /// The song's tuning against A440 (cents): the guitar plays that much sharp.
    cents: f64,
    duration: f64,
    has_track: bool,
    /// Every track sent to the player, released here once the callback no longer holds it.
    tracks: Vec<Arc<Track>>,
    generation: u32,
    /// Song time asked by the last load or seek, until the player clock reflects it.
    target: f64,
    sent: usize,
    events: Vec<UiEvent>,
    history: TapHistory,
    verifier: ChordVerifier,
    scratch: Vec<f32>,
    attacks: VecDeque<f64>,
    last_score: Score,
    ended: bool,
    tuner: bool,
    /// Tuner mode's own analysis, on a long window of the input.
    tuning: Option<TunerAnalyzer>,
    /// Demo mode: the game plays the notes itself.
    autopilot: bool,
    /// Input sample of the last pitch sent in tuner mode.
    last_pitch: u64,
    calibration: Option<Calibration>,
    /// Playback speed (track seconds per second).
    rate: f64,
    riff: Option<RiffLoop>,
    /// Loop count of the player clock already turned into passes.
    loops_seen: u32,
    /// What was set on the audio engine, applied again to a new one (another output).
    applied: Applied,
}

#[derive(Clone, Copy, Default)]
struct Applied {
    input_gain: Option<f32>,
    monitor_gain: Option<f32>,
    volume: Option<f32>,
    /// Gain of the song's own guitar stem, for every track loaded.
    stem: f32,
    chain: Option<ChainParams>,
}

impl<B: AudioBackend> Session<B> {
    pub fn new(mut backend: B, cfg: SessionConfig) -> Self {
        let sample_rate = backend.monitor().sample_rate();
        Session {
            backend,
            cfg,
            sample_rate,
            game: None,
            offset: 0.0,
            cents: 0.0,
            duration: 0.0,
            has_track: false,
            tracks: Vec::new(),
            generation: 0,
            target: 0.0,
            sent: 0,
            events: Vec::new(),
            history: TapHistory::new(1 << 18),
            verifier: ChordVerifier::new(ChordConfig::default()),
            scratch: Vec::new(),
            attacks: VecDeque::new(),
            last_score: Score::default(),
            ended: false,
            tuner: false,
            tuning: None,
            autopilot: false,
            last_pitch: 0,
            calibration: None,
            rate: 1.0,
            riff: None,
            loops_seen: 0,
            applied: Applied::default(),
        }
    }

    /// Plays through another audio engine (another output device): the song and the clock
    /// start over (load again), the guitar settings, amp, volume and speed carry over. Returns
    /// the previous engine, to drop once this one runs.
    pub fn replace_backend(&mut self, backend: B) -> B {
        let old = std::mem::replace(&mut self.backend, backend);
        self.sample_rate = self.backend.monitor().sample_rate();
        // The new device's input stream counts from zero again.
        self.history = TapHistory::new(1 << 18);
        self.set_tuner(self.tuner);
        self.tracks.clear();
        self.game = None;
        self.has_track = false;
        self.calibration = None;
        self.riff = None;
        self.generation = self.generation.wrapping_add(1);
        self.loops_seen = 0;
        self.restart_view(0.0);
        let (a, rate) = (self.applied, self.rate);
        let monitor = self.backend.monitor();
        if let Some(g) = a.input_gain {
            monitor.set_input_gain(g);
        }
        if let Some(g) = a.monitor_gain {
            monitor.set_monitor_gain(g);
        }
        if let Some(g) = a.volume {
            monitor.send(PlayerCommand::Gain(g));
        }
        if let Some(c) = a.chain {
            monitor.set_chain(c);
        }
        if rate != 1.0 {
            monitor.send(PlayerCommand::Rate(rate));
        }
        old
    }

    /// The guitar's sound: amp model, knobs, effects.
    pub fn set_chain(&mut self, params: ChainParams) {
        self.applied.chain = Some(params);
        self.backend.monitor().set_chain(params);
    }

    pub fn backend(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn chart(&self) -> Option<&Chart> {
        self.game.as_ref().map(Game::chart)
    }

    pub fn score(&self) -> Option<Score> {
        self.game.as_ref().map(Game::score)
    }

    /// Loads a song and its backing track, stopped at song time `start` (negative: count-in).
    pub fn load(&mut self, chart: Chart, track: Option<Track>, start: f64) {
        let track = track.map(Arc::new);
        if let Some(t) = &track {
            t.set_stem_gain(self.applied.stem);
        }
        self.tracks.extend(track.clone());
        self.has_track = track.is_some();
        self.offset = chart.audio.offset;
        self.cents = chart.arrangement.tuning.cents;
        self.duration = chart.meta.duration;
        self.generation = self.generation.wrapping_add(1);
        let frame = self.frame_of(start);
        self.set_rate(1.0);
        self.riff = None;
        self.loops_seen = 0;
        self.backend.monitor().send(PlayerCommand::Load { track, frame, generation: self.generation });
        let mut game = Game::new(chart, self.cfg.game);
        game.restart_at(start);
        self.game = Some(game);
        self.calibration = None;
        self.restart_view(start);
    }

    /// Plays the click track of `plan` and measures how late the player's strums come.
    pub fn calibrate(&mut self, plan: CalibrationPlan) {
        let track = Arc::new(plan.track(self.sample_rate as u32));
        self.tracks.push(track.clone());
        self.has_track = true;
        self.offset = 0.0;
        self.duration = plan.end();
        self.game = None;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.set_rate(1.0);
        self.riff = None;
        self.loops_seen = 0;
        let monitor = self.backend.monitor();
        monitor.send(PlayerCommand::Load { track: Some(track), frame: 0, generation });
        monitor.send(PlayerCommand::Play);
        self.restart_view(0.0);
        self.events.push(UiEvent::CalibrationStart { first: plan.first, period: plan.period, count_in: plan.count_in, clicks: plan.clicks });
        self.calibration = Some(Calibration::new(plan));
    }

    /// Levels each phrase (by name) starts at in the loaded song: what the player reached last
    /// time (dynamic difficulty picks up from there).
    pub fn set_levels(&mut self, levels: HashMap<String, u32>) {
        if let Some(game) = &mut self.game {
            game.set_levels(levels);
        }
    }

    /// Tuner mode: the pitch of the ringing string is sent continuously.
    /// Demo mode: the game plays every note itself, on time (the guitar is still heard).
    pub fn set_autopilot(&mut self, on: bool) {
        self.autopilot = on;
    }

    pub fn set_tuner(&mut self, on: bool) {
        self.tuner = on;
        self.last_pitch = 0;
        self.tuning = on.then(|| TunerAnalyzer::new(TunerConfig { sample_rate: self.sample_rate as f32, ..TunerConfig::default() }));
    }

    /// Tuner mode: the ringing string's pitch, measured on the last 170 ms of input, every 20 ms.
    fn tune(&mut self) {
        let Some(analyzer) = &mut self.tuning else { return };
        let end = self.history.end();
        let (every, window) = ((PITCH_EVERY * self.sample_rate) as u64, analyzer.window());
        if end < window as u64 || (end >= self.last_pitch && end < self.last_pitch + every) {
            return;
        }
        self.last_pitch = end;
        self.scratch.resize(window, 0.0);
        if !self.history.read(end - window as u64, &mut self.scratch) {
            return;
        }
        if let Some(r) = analyzer.analyze(&self.scratch) {
            self.events.push(UiEvent::Pitch { midi: r.midi as f32, clarity: r.clarity });
        }
    }

    /// Latency compensation (s), from the calibration; applies to the song being played too.
    pub fn set_input_offset(&mut self, seconds: f64) {
        self.cfg.game.input_offset = seconds;
        if let Some(game) = &mut self.game {
            game.set_input_offset(seconds);
        }
    }

    /// Linear gain of the guitar input.
    pub fn set_input_gain(&mut self, gain: f32) {
        self.applied.input_gain = Some(gain);
        self.backend.monitor().set_input_gain(gain);
    }

    /// Linear gain of the guitar heard in the headphones (only with the USB guitar cable).
    pub fn set_monitor_gain(&mut self, gain: f32) {
        self.applied.monitor_gain = Some(gain);
        self.backend.monitor().set_monitor_gain(gain);
    }

    /// Loudest guitar sample since the last call (level meter).
    pub fn take_peak(&mut self) -> f32 {
        self.backend.monitor().take_peak()
    }

    pub fn play(&mut self) {
        self.backend.monitor().send(PlayerCommand::Play);
    }

    pub fn pause(&mut self) {
        self.backend.monitor().send(PlayerCommand::Pause);
    }

    /// Moves to song time `t`: judging starts over from there, phrase levels are kept.
    pub fn seek(&mut self, t: f64) {
        self.generation = self.generation.wrapping_add(1);
        self.loops_seen = 0;
        let frame = self.frame_of(t);
        self.backend.monitor().send(PlayerCommand::Seek { frame, generation: self.generation });
        if let Some(game) = &mut self.game {
            game.restart_at(t);
        }
        self.restart_view(t);
    }

    /// Playback speed (1: as recorded), the pitch unchanged: the song clock, the notes and the
    /// judging follow it.
    pub fn set_rate(&mut self, rate: f64) {
        self.rate = rate.clamp(MIN_RATE, MAX_RATE);
        self.backend.monitor().send(PlayerCommand::Rate(self.rate));
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Loop Trainer on (`Some`: plays the loop from its start, a `Pass` event each time it goes
    /// back) or off (`None`: plays on from where it is, every note judged from the next seek).
    pub fn set_riff(&mut self, riff: Option<RiffLoop>) {
        self.riff = riff;
        let frames = riff.map(|r| (self.frame_of(r.start), self.frame_of(r.end)));
        self.backend.monitor().send(PlayerCommand::Loop(frames));
        let (from, to) = riff.map_or((f64::NEG_INFINITY, f64::INFINITY), |r| (r.from, r.to));
        if let Some(game) = &mut self.game {
            game.set_range(from, to);
        }
        if let Some(r) = riff {
            self.seek(r.start);
        }
    }

    pub fn riff(&self) -> Option<RiffLoop> {
        self.riff
    }

    /// Difficulty level every phrase starts at, from the next load.
    pub fn set_start_level(&mut self, level: u32) {
        self.cfg.game.start_level = level;
    }

    /// Dynamic difficulty from the next load; off, every phrase stays at the start level
    /// (Score Attack).
    pub fn set_dynamic(&mut self, on: bool) {
        self.cfg.game.dynamic_difficulty = on;
    }

    /// Difficulty changed during the song: from the next phrase, and for the next loads.
    pub fn set_difficulty(&mut self, dynamic: bool, level: u32) {
        self.cfg.game.dynamic_difficulty = dynamic;
        self.cfg.game.start_level = level;
        if let Some(game) = &mut self.game {
            game.set_difficulty(dynamic, level);
        }
    }

    /// Linear gain of the backing track.
    pub fn set_volume(&mut self, gain: f32) {
        self.applied.volume = Some(gain);
        self.backend.monitor().send(PlayerCommand::Gain(gain));
    }

    /// Linear gain of the song's own guitar, when its track has it as a stem (silent by
    /// default): heard at once, and kept for the next songs.
    pub fn set_stem_volume(&mut self, gain: f32) {
        self.applied.stem = gain;
        for t in &self.tracks {
            t.set_stem_gain(gain);
        }
    }

    /// Song time heard now (s).
    pub fn time(&mut self) -> f64 {
        let now = self.backend.now_ns() as f64;
        match self.synced_clock() {
            Some(c) => c.time_at(now, self.sample_rate) - self.offset,
            None => self.target,
        }
    }

    pub fn playing(&mut self) -> bool {
        self.synced_clock().is_some_and(|c| c.playing)
    }

    pub fn take_events(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.events)
    }

    /// Reads the audio engine and judges: call every few milliseconds.
    pub fn update(&mut self) {
        self.tracks.retain(|t| Arc::strong_count(t) > 1);
        let (sr, offset) = (self.sample_rate, self.offset);
        let history = &mut self.history;
        self.backend.monitor().drain_tap(|b| history.push(b.first, b.samples()));
        let mut onsets = Vec::new();
        let mut notes = Vec::new();
        self.backend.monitor().poll_events(|e| match e {
            TrackerEvent::Onset { sample } => onsets.push(sample),
            TrackerEvent::NoteOn { onset_sample, midi, cents, .. } => notes.push((onset_sample, midi, cents)),
            _ => {}
        });
        self.tune();
        // Out of a song, what the guitar plays still shows on the strings.
        if !self.synced_clock().is_some_and(|c| c.playing) {
            self.events.extend(notes.iter().map(|&(_, midi, cents)| UiEvent::Played { midi, cents }));
            if notes.is_empty() && !onsets.is_empty() {
                self.events.push(UiEvent::Strum);
            }
        }
        // Until the player applied the last load or seek, what was heard belongs to the past.
        let Some(clock) = self.synced_clock() else { return };
        if clock.loops != self.loops_seen {
            self.loops_seen = clock.loops;
            self.next_pass();
        }
        let now = clock.time_at(self.backend.now_ns() as f64, sr) - offset;
        let monitor = self.backend.monitor();
        let song = |m: &Monitor, sample: u64| m.input_time_ns(sample as f64).map(|h| clock.time_at(h, sr) - offset);
        let strums: Vec<f64> = onsets.iter().filter_map(|&s| song(monitor, s)).collect();
        self.attacks.extend(strums.iter().copied());
        while self.attacks.front().is_some_and(|&t| t < now - ATTACK_MEMORY) {
            self.attacks.pop_front();
        }
        let heard: Vec<(f64, i32, f32)> =
            notes.iter().filter_map(|&(s, midi, cents)| song(monitor, s).map(|t| (t, midi, cents))).collect();
        // A note's attack is a strum too when the onset detector missed it (a chord over a
        // ringing one).
        self.attacks.extend(heard.iter().map(|h| h.0));

        if let Some(cal) = self.calibration.as_mut() {
            for &t in &strums {
                if let Some((index, error)) = cal.strum(t) {
                    self.events.push(UiEvent::CalibrationHit { index, error });
                }
            }
            if clock.ended || now > cal.plan.end() {
                let r = cal.result();
                self.events.push(UiEvent::Calibrated { offset: r.offset, spread: r.spread, hits: r.hits, total: r.total });
                self.calibration = None;
            }
        }

        let Some(game) = self.game.as_mut() else {
            self.events.extend(heard.into_iter().map(|(time, midi, cents)| UiEvent::Heard { time, midi, cents }));
            return;
        };
        game.activate_until(now);
        flush_expected(game, &mut self.sent, &mut self.events);
        if self.autopilot && clock.playing {
            self.events.extend(game.autoplay(now).into_iter().map(UiEvent::from));
        }
        if clock.playing {
            for &t in &strums {
                self.events.extend(game.on_strum(t).map(UiEvent::from));
            }
        }
        for (time, midi, cents) in heard {
            self.events.push(UiEvent::Heard { time, midi, cents });
            if clock.playing {
                self.events.extend(game.on_note(time, retuned(midi, cents, self.cents)).map(UiEvent::from));
            }
        }

        if clock.playing {
            let (w, window) = (self.cfg.game.hit_window, self.cfg.chord_window);
            let a4 = self.cfg.a4 * 2f32.powf(self.cents as f32 / 1200.0);
            for (index, time, pitches) in game.chords_ready(now, window) {
                let attack = self
                    .attacks
                    .iter()
                    .copied()
                    .filter(|a| (a - time).abs() <= w)
                    .min_by(|x, y| (x - time).abs().total_cmp(&(y - time).abs()));
                let mut verify = |t: f64| {
                    strum_audio(monitor, &clock, &self.history, &mut self.scratch, t + offset, window, sr)
                        .is_some_and(|audio| self.verifier.verify(audio, &pitches, a4).accepted)
                };
                // The strum's own audio as soon as it is in; failing that, at the end of the
                // window, the chord anywhere in it (the tracker may not have noticed the strum).
                let verdict = match attack {
                    Some(a) if now >= a + window + TAP_DELAY && verify(a) => Some((true, Some(a))),
                    _ if now >= time + w + window + TAP_DELAY => {
                        let steps = (2.0 * w / SCAN_STEP).round() as usize;
                        let found = (0..=steps).map(|k| time - w + k as f64 * SCAN_STEP).find(|&t| verify(t));
                        Some((found.is_some(), attack.or(found)))
                    }
                    _ => None,
                };
                if let Some((accepted, at)) = verdict {
                    self.events.extend(game.on_chord(index, accepted, at).map(UiEvent::from));
                }
            }
        }

        self.events.extend(game.advance(now).into_iter().map(UiEvent::from));
        flush_expected(game, &mut self.sent, &mut self.events);
        let score = game.score();
        if score != self.last_score {
            self.last_score = score;
            self.events.push(UiEvent::Score { score: score.into() });
        }
        let finished = if self.has_track { clock.ended } else { now > self.duration + 1.0 };
        if finished && !self.ended {
            self.ended = true;
            self.events.push(UiEvent::End);
        }
    }

    /// The loop went back to its start: the pass is over (whatever is still pending counts as
    /// missed), its score goes out, and the notes are announced again for the next one.
    fn next_pass(&mut self) {
        let (Some(riff), Some(game)) = (self.riff, self.game.as_mut()) else { return };
        self.events.extend(game.advance(f64::INFINITY).into_iter().map(UiEvent::from));
        self.events.push(UiEvent::Pass { score: game.score().into() });
        game.restart_at(riff.start);
        self.restart_view(riff.start);
    }

    fn frame_of(&self, t: f64) -> i64 {
        ((t + self.offset) * self.sample_rate).round() as i64
    }

    fn restart_view(&mut self, t: f64) {
        self.target = t;
        self.sent = 0;
        self.attacks.clear();
        self.last_score = Score::default();
        self.ended = false;
        self.events.push(UiEvent::Reset);
    }

    /// The player clock, once it reflects the last load or seek.
    fn synced_clock(&mut self) -> Option<PlayerClock> {
        let c = self.backend.monitor().clock();
        (c.valid && c.generation == self.generation).then_some(c)
    }
}

/// Announces the notes made visible since the last call, one event per phrase iteration.
fn flush_expected(game: &Game, sent: &mut usize, events: &mut Vec<UiEvent>) {
    let expected = game.expected();
    while *sent < expected.len() {
        let iteration = expected[*sent].iteration;
        let level = game.iteration_level(iteration).unwrap_or(0);
        let mut items = Vec::new();
        while *sent < expected.len() && expected[*sent].iteration == iteration {
            items.push(UiExpected::new(*sent, &expected[*sent], level));
            *sent += 1;
        }
        events.push(UiEvent::Expected { iteration, level, items });
    }
}

/// The guitar audio of a strum heard at track time `t`, `window` seconds of it.
fn strum_audio<'a>(
    monitor: &Monitor,
    clock: &PlayerClock,
    history: &TapHistory,
    scratch: &'a mut Vec<f32>,
    t: f64,
    window: f64,
    sr: f64,
) -> Option<&'a [f32]> {
    let first = monitor.input_index_at(clock.host_at(t, sr))?;
    if first < 0.0 {
        return None;
    }
    scratch.resize((window * sr) as usize, 0.0);
    history.read(first.round() as u64, scratch).then_some(&scratch[..])
}

/// The chart note a heard one stands for, the guitar tuned `song_cents` sharp like the
/// recording: `midi` + `cents` is the pitch heard.
fn retuned(midi: i32, cents: f32, song_cents: f64) -> i32 {
    if song_cents == 0.0 {
        return midi;
    }
    (midi as f64 + (cents as f64 - song_cents) / 100.0).round() as i32
}
