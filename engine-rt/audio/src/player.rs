//! Backing-track playback inside the output callback, and the song clock it publishes.
//!
//! The control thread sends [`PlayerCommand`]s through a wait-free ring. Tracks travel as
//! `Arc`s and the sender keeps its own reference until the callback has let go of one, so the
//! callback never allocates nor frees. After each callback the track position of its first frame
//! and the host time at which that frame reaches the listener are published through a seqlock:
//! song time can then be computed for any host time, which is how a guitar attack captured by
//! the input device gets its place in the song.

use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{fence, AtomicU32, AtomicU64};
use std::sync::Arc;

/// A backing track: interleaved stereo at the engine sample rate. It may carry the song's own
/// guitar as a separate stem, mixed in at a gain that can change while it plays.
pub struct Track {
    samples: Vec<f32>,
    stem: Option<Vec<f32>>,
    /// Linear gain of the stem (f32 bits).
    stem_gain: AtomicU32,
}

/// `data` interleaved with `channels` channels as stereo: mono is doubled, channels past two
/// dropped.
fn stereo(data: &[f32], channels: usize) -> Vec<f32> {
    match channels.max(1) {
        1 => data.iter().flat_map(|&v| [v, v]).collect(),
        2 => data.to_vec(),
        n => data.chunks_exact(n).flat_map(|f| [f[0], f[1]]).collect(),
    }
}

impl Track {
    /// `data` interleaved with `channels` channels: mono is doubled, channels past two dropped.
    pub fn from_interleaved(data: &[f32], channels: usize) -> Track {
        Track { samples: stereo(data, channels), stem: None, stem_gain: AtomicU32::new(0) }
    }

    /// Adds the song's guitar (interleaved, `channels` channels, same sample rate), silent until
    /// [`Track::set_stem_gain`]. The track keeps its length: the stem is cut or padded to it.
    pub fn with_stem(mut self, data: &[f32], channels: usize) -> Track {
        let mut stem = stereo(data, channels);
        stem.resize(self.samples.len(), 0.0);
        self.stem = Some(stem);
        self
    }

    pub fn has_stem(&self) -> bool {
        self.stem.is_some()
    }

    /// Linear gain of the guitar stem; takes effect at once, even while playing.
    pub fn set_stem_gain(&self, gain: f32) {
        self.stem_gain.store(gain.max(0.0).to_bits(), Relaxed);
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / 2
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Frame `i` (left, right), silence outside the track.
    #[inline]
    fn frame(&self, i: i64) -> (f32, f32) {
        if i < 0 || i as usize >= self.frames() {
            return (0.0, 0.0);
        }
        let k = 2 * i as usize;
        match &self.stem {
            Some(stem) => {
                let g = f32::from_bits(self.stem_gain.load(Relaxed));
                (self.samples[k] + g * stem[k], self.samples[k + 1] + g * stem[k + 1])
            }
            None => (self.samples[k], self.samples[k + 1]),
        }
    }

    #[inline]
    fn mono(&self, i: i64) -> f32 {
        let (l, r) = self.frame(i);
        0.5 * (l + r)
    }
}

pub enum PlayerCommand {
    /// Replaces the track (none: a silent clock), stops, moves to `frame`, ends any loop.
    Load { track: Option<Arc<Track>>, frame: i64, generation: u32 },
    /// Moves to `frame` (negative: count-in silence before the track).
    Seek { frame: i64, generation: u32 },
    Play,
    Pause,
    /// Linear gain of the track.
    Gain(f32),
    /// Playback speed (track seconds per second, within [`MIN_RATE`, `MAX_RATE`]); the pitch
    /// does not change.
    Rate(f64),
    /// Plays track frames `start..end` over and over (Loop Trainer); `None` plays on.
    Loop(Option<(i64, i64)>),
}

/// Slowest playback speed.
pub const MIN_RATE: f64 = 0.25;
/// Fastest playback speed.
pub const MAX_RATE: f64 = 2.0;

/// Player state at the start of the last output callback.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerClock {
    /// Host time (ns) at which `frame` reaches the listener.
    pub heard_ns: u64,
    /// Track position in frames, negative during a count-in.
    pub frame: i64,
    pub playing: bool,
    /// The track played to its end.
    pub ended: bool,
    /// Generation of the last load or seek applied: callers ignore the clock until it matches.
    pub generation: u32,
    /// False until the first output callback.
    pub valid: bool,
    /// Track seconds per second.
    pub rate: f64,
    /// Times the loop went back to its start since the last load or seek.
    pub loops: u32,
}

impl PlayerClock {
    /// Track time (s) heard at host time `ns`.
    pub fn time_at(&self, ns: f64, sample_rate: f64) -> f64 {
        let t = self.frame as f64 / sample_rate;
        if self.playing { t + (ns - self.heard_ns as f64) * 1e-9 * self.rate } else { t }
    }

    /// Host time (ns) at which track time `t` is heard, the player running.
    pub fn host_at(&self, t: f64, sample_rate: f64) -> f64 {
        self.heard_ns as f64 + (t - self.frame as f64 / sample_rate) / self.rate * 1e9
    }
}

/// Seqlock holding the clock: written by the output callback only, read anywhere.
#[derive(Default)]
pub(crate) struct ClockCell {
    seq: AtomicU64,
    heard_ns: AtomicU64,
    frame: AtomicU64,
    flags: AtomicU64,
    /// Rate (f64 bits).
    rate: AtomicU64,
    loops: AtomicU64,
}

impl ClockCell {
    pub(crate) fn publish(&self, c: &PlayerClock) {
        let s = self.seq.load(Relaxed);
        self.seq.store(s.wrapping_add(1), Relaxed);
        fence(Release);
        self.heard_ns.store(c.heard_ns, Relaxed);
        self.frame.store(c.frame as u64, Relaxed);
        let flags = c.playing as u64 | (c.ended as u64) << 1 | (c.valid as u64) << 2 | (c.generation as u64) << 32;
        self.flags.store(flags, Relaxed);
        self.rate.store(c.rate.to_bits(), Relaxed);
        self.loops.store(c.loops as u64, Relaxed);
        self.seq.store(s.wrapping_add(2), Release);
    }

    pub(crate) fn read(&self) -> PlayerClock {
        loop {
            let s = self.seq.load(Acquire);
            if s & 1 == 1 {
                std::hint::spin_loop();
                continue;
            }
            let heard_ns = self.heard_ns.load(Relaxed);
            let frame = self.frame.load(Relaxed) as i64;
            let flags = self.flags.load(Relaxed);
            let rate = f64::from_bits(self.rate.load(Relaxed));
            let loops = self.loops.load(Relaxed) as u32;
            fence(Acquire);
            if self.seq.load(Relaxed) == s {
                return PlayerClock {
                    heard_ns,
                    frame,
                    playing: flags & 1 != 0,
                    ended: flags & 2 != 0,
                    valid: flags & 4 != 0,
                    generation: (flags >> 32) as u32,
                    // Before the first callback the cell is zeroed.
                    rate: if rate > 0.0 { rate } else { 1.0 },
                    loops,
                };
            }
        }
    }
}

/// The player as the output callback sees it.
pub(crate) struct Player {
    commands: rtrb::Consumer<PlayerCommand>,
    track: Option<Arc<Track>>,
    /// Track position (frames) of the next output frame, as the clock tells it.
    pos: f64,
    rate: f64,
    looping: Option<(i64, i64)>,
    loops: u32,
    playing: bool,
    ended: bool,
    gain: f32,
    generation: u32,
    stretch: Stretch,
}

impl Player {
    pub(crate) fn new(commands: rtrb::Consumer<PlayerCommand>, sample_rate: f64) -> Player {
        Player {
            commands,
            track: None,
            pos: 0.0,
            rate: 1.0,
            looping: None,
            loops: 0,
            playing: false,
            ended: false,
            gain: 1.0,
            generation: 0,
            stretch: Stretch::new(sample_rate),
        }
    }

    /// Applies pending commands and adds the track to `out` (interleaved, `channels` per frame).
    /// Returns the clock of this callback, whose first frame is heard at `heard_ns`.
    pub(crate) fn render(&mut self, out: &mut [f32], channels: usize, heard_ns: u64) -> PlayerClock {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                PlayerCommand::Load { track, frame, generation } => {
                    // The sender still holds the previous track: this only drops a reference.
                    self.track = track;
                    self.pos = frame as f64;
                    self.playing = false;
                    self.ended = false;
                    self.generation = generation;
                    self.loops = 0;
                    self.looping = None;
                    self.stretch.join(Join::Cut);
                }
                PlayerCommand::Seek { frame, generation } => {
                    self.pos = frame as f64;
                    self.ended = false;
                    self.generation = generation;
                    self.loops = 0;
                    self.stretch.join(Join::Cut);
                }
                PlayerCommand::Play => self.playing = !self.ended,
                PlayerCommand::Pause => self.playing = false,
                PlayerCommand::Gain(g) => self.gain = g,
                PlayerCommand::Rate(r) => {
                    let r = if (r - 1.0).abs() < 1e-6 { 1.0 } else { r.clamp(MIN_RATE, MAX_RATE) };
                    if r != self.rate {
                        self.rate = r;
                        self.stretch.join(Join::Blend);
                    }
                }
                PlayerCommand::Loop(bounds) => self.looping = bounds.filter(|(a, b)| b > a),
            }
        }
        let clock = PlayerClock {
            heard_ns,
            frame: self.pos.floor() as i64,
            playing: self.playing,
            ended: self.ended,
            generation: self.generation,
            valid: true,
            rate: self.rate,
            loops: self.loops,
        };
        if !self.playing {
            return clock;
        }
        let gain = self.gain;
        for frame in out.chunks_mut(channels.max(1)) {
            if let Some((a, b)) = self.looping
                && self.pos >= b as f64
            {
                self.pos = a as f64 + (self.pos - b as f64);
                if self.pos >= b as f64 {
                    self.pos = a as f64;
                }
                self.loops = self.loops.wrapping_add(1);
                self.stretch.join(Join::Wrap);
            }
            if let Some(track) = &self.track {
                let (l, r) = self.stretch.next(track, self.pos, self.rate);
                if channels == 1 {
                    frame[0] += 0.5 * (l + r) * gain;
                } else {
                    frame[0] += l * gain;
                    frame[1] += r * gain;
                }
            }
            self.pos += self.rate;
        }
        if let Some(track) = &self.track
            && self.looping.is_none()
            && self.pos >= track.frames() as f64
        {
            self.playing = false;
            self.ended = true;
        }
        clock
    }
}

/// How the next segment joins the audio played so far.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Join {
    /// Starts dry (load, seek): what was playing stops.
    Cut,
    /// Continues the previous segment where it matches best (normal flow below speed 1).
    Match,
    /// Crossfades from the previous segment (speed change).
    Blend,
    /// Crossfades at equal power from unrelated audio (loop back to its start).
    Wrap,
}

/// Crossfade of a loop's end into its start (s).
const WRAP_FADE: f64 = 0.012;

/// Segment sizes for a speed, as SoundTouch picks them: longer segments when slower.
struct Shape {
    /// Frames played per segment, its crossfade included.
    seg: usize,
    /// Range searched for the best continuation (frames).
    seek: usize,
    /// Crossfade between segments (frames).
    overlap: usize,
}

fn shape(rate: f64, sr: f64) -> Shape {
    let x = rate.clamp(0.5, 2.0) - 0.5;
    let sequence = 0.090 - x * (0.050 / 1.5);
    let seek = 0.020 - x * (0.005 / 1.5);
    let overlap = 0.008;
    Shape { seg: ((sequence - overlap) * sr) as usize, seek: (seek * sr) as usize, overlap: (overlap * sr) as usize }
}

/// Speed change without pitch change (SOLA, after SoundTouch's TDStretch): the track is played
/// in segments of a few tens of milliseconds, each copied as is and crossfaded into the next.
/// Slower than 1, each segment starts back near the clock, so parts of the music are heard
/// twice; the start is chosen in a small range around there where it best continues the
/// previous segment (normalised cross-correlation, both channels moved alike). At speed 1 the
/// track is played as is. Buffers are allocated up front: the callback never allocates.
struct Stretch {
    sr: f64,
    /// Next track frame the current segment reads.
    src: i64,
    /// Frames left in the current segment (`usize::MAX` at speed 1).
    left: usize,
    next_join: Join,
    /// Crossfade from where the previous segment would have gone on.
    fade_src: i64,
    fade_left: usize,
    fade_len: usize,
    fade_power: bool,
    /// Search scratch: mono of the previous segment's continuation, of the candidates.
    reference: Vec<f32>,
    candidates: Vec<f32>,
}

impl Stretch {
    fn new(sr: f64) -> Stretch {
        let widest = shape(MIN_RATE, sr);
        Stretch {
            sr,
            src: 0,
            left: 0,
            next_join: Join::Cut,
            fade_src: 0,
            fade_left: 0,
            fade_len: 1,
            fade_power: false,
            reference: vec![0.0; widest.overlap + 8],
            candidates: vec![0.0; widest.seek + widest.overlap + 8],
        }
    }

    /// The next frame starts a new segment, joined this way.
    fn join(&mut self, join: Join) {
        self.left = 0;
        // A cut stays a cut until played: a seek followed by a speed change starts dry.
        if self.next_join != Join::Cut || join == Join::Cut {
            self.next_join = join;
        }
    }

    /// The frame for track position `pos` (the clock) at speed `rate`.
    fn next(&mut self, track: &Track, pos: f64, rate: f64) -> (f32, f32) {
        if self.left == 0 {
            self.start_segment(track, pos, rate);
        }
        let (mut l, mut r) = track.frame(self.src);
        if self.fade_left > 0 {
            let x = 1.0 - (self.fade_left as f32 - 0.5) / self.fade_len as f32;
            let (fade_in, fade_out) = if self.fade_power {
                let a = x * std::f32::consts::FRAC_PI_2;
                (a.sin(), a.cos())
            } else {
                (x, 1.0 - x)
            };
            let (fl, fr) = track.frame(self.fade_src);
            l = l * fade_in + fl * fade_out;
            r = r * fade_in + fr * fade_out;
            self.fade_src += 1;
            self.fade_left -= 1;
        }
        self.src += 1;
        self.left -= 1;
        (l, r)
    }

    fn start_segment(&mut self, track: &Track, pos: f64, rate: f64) {
        let join = std::mem::replace(&mut self.next_join, Join::Match);
        let previous = self.src;
        let (start, overlap) = if rate == 1.0 {
            self.left = usize::MAX;
            (pos.round() as i64, (0.008 * self.sr) as usize)
        } else {
            let s = shape(rate, self.sr);
            self.left = s.seg;
            // Within a segment the track runs ahead of the clock by up to (1 − rate)·seg:
            // starting half of that early keeps what is heard centred on the clock.
            let target = (pos - (1.0 - rate) * s.seg as f64 / 2.0).round() as i64;
            let matched = matches!(join, Join::Match | Join::Blend);
            let start = if matched { self.best_match(track, target, previous, &s) } else { target };
            (start, s.overlap)
        };
        self.src = start;
        self.fade_src = previous;
        (self.fade_len, self.fade_power) = match join {
            Join::Cut => (0, false),
            Join::Match | Join::Blend => (overlap, false),
            Join::Wrap => ((WRAP_FADE * self.sr) as usize, true),
        };
        self.fade_len = self.fade_len.max(1);
        self.fade_left = if join == Join::Cut { 0 } else { self.fade_len };
    }

    /// Start, within `seek` frames centred on `target`, of the segment that best continues the
    /// audio from `previous`.
    fn best_match(&mut self, track: &Track, target: i64, previous: i64, s: &Shape) -> i64 {
        let n = s.overlap;
        let half = (s.seek / 2) as i64;
        let first = target - half;
        let count = 2 * half as usize + 1;
        for (i, m) in self.reference[..n].iter_mut().enumerate() {
            *m = track.mono(previous + i as i64);
        }
        let energy = dot(&self.reference[..n], &self.reference[..n]);
        if energy < 1e-9 {
            return target;
        }
        for (i, m) in self.candidates[..count + n].iter_mut().enumerate() {
            *m = track.mono(first + i as i64);
        }
        let (reference, candidates) = (&self.reference[..n], &self.candidates[..count + n]);
        let score = |k: usize| {
            let c = &candidates[k..k + n];
            let corr = dot(reference, c) / (energy * dot(c, c) + 1e-12).sqrt();
            // Slight preference for the centre: less wander around the clock.
            let u = (k as f32 - half as f32) / half.max(1) as f32;
            (corr + 0.1) * (1.0 - 0.25 * u * u)
        };
        // Coarse then fine: the correlation of music is smooth over a few samples.
        let mut best = (half as usize, f32::MIN);
        for k in (0..count).step_by(4) {
            let v = score(k);
            if v > best.1 {
                best = (k, v);
            }
        }
        for k in best.0.saturating_sub(3)..=(best.0 + 3).min(count - 1) {
            let v = score(k);
            if v > best.1 {
                best = (k, v);
            }
        }
        first + best.0 as i64
    }
}

/// Dot product in eight lanes (vectorised).
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut acc = [0f32; 8];
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let tail: f32 = ca.remainder().iter().zip(cb.remainder()).map(|(x, y)| x * y).sum();
    for (x, y) in ca.zip(cb) {
        for i in 0..8 {
            acc[i] += x[i] * y[i];
        }
    }
    acc.iter().sum::<f32>() + tail
}

/// Samples per [`TapBlock`].
pub const TAP_BLOCK: usize = 256;

/// Raw guitar samples (before the FX chain) for analyses that need audio, such as chord
/// verification. `first` is the index of `data[0]` in the input stream, so a reader that fell
/// behind sees the gap.
#[derive(Clone, Copy)]
pub struct TapBlock {
    pub first: u64,
    pub len: usize,
    pub data: [f32; TAP_BLOCK],
}

impl Default for TapBlock {
    fn default() -> Self {
        TapBlock { first: 0, len: 0, data: [0.0; TAP_BLOCK] }
    }
}

impl TapBlock {
    pub fn samples(&self) -> &[f32] {
        &self.data[..self.len]
    }
}
