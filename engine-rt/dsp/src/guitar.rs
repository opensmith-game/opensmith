//! A virtual electric guitar as its cable carries it: six strings, each sounding one note at a
//! time (the next note on a string damps the one before), the others ringing on; notes picked,
//! hammered on or pulled off, slid, bent, with vibrato, palm-muted or dead, natural harmonics;
//! strings a few cents out of tune; pickup hum and hiss. Offline (whole songs) and deterministic:
//! the engine can be measured on playing whose every note is known.

use std::f64::consts::{PI, TAU};

use crate::synth::Pcg32;

/// How a note is set ringing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Excitation {
    /// Picked, `level` 0..1, at `pos` (fraction of the string from the bridge).
    Pick { level: f32, pos: f32 },
    /// Hammer-on or pull-off: the fretting hand alone, softer, no pick click.
    Legato { level: f32 },
    /// A dead note: the fretting hand mutes the string, the pick only clicks and thumps.
    Dead { level: f32 },
    /// A natural harmonic at the node dividing the string in `n` (12th fret: 2, 7th: 3, 5th: 4).
    Harmonic { level: f32, n: u32 },
}

/// How a recorded note was played (`Samples`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Plain,
    PalmMute,
    Vibrato,
    Pinch,
    Harmonic,
}

/// Notes recorded from real guitars, to play instead of synthesising them: real attacks,
/// timbres, decays and noises.
pub trait Samples: Sync {
    /// A recording of `string` at `fret` played `kind`, the `choice`-th when there are several
    /// (several guitars); from its attack.
    fn note(&self, string: usize, fret: i32, kind: Kind, choice: u32) -> Option<&[f32]>;
}

/// One note as the guitar sounds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    pub string: usize,
    /// Seconds from the start of the rendering.
    pub start: f64,
    /// When the string is damped: the next note on it, the finger lifted, the hand muting.
    pub end: f64,
    /// Pitch at the attack, fractional MIDI (a string a few cents out of tune). For a harmonic,
    /// the open string's.
    pub midi: f64,
    /// Pitch offsets over time in semitones, (seconds from the attack, offset), linear between
    /// points and held after the last: a bend, a slide. Empty: steady.
    pub glide: Vec<(f64, f64)>,
    /// Vibrato depth in semitones, at 5.5 Hz.
    pub vibrato: f64,
    pub excitation: Excitation,
    /// Palm-muted: the picking hand damps the string at the bridge.
    pub palm: bool,
    /// The recording to play instead, given recorded notes: (fret, how it is played, which one).
    pub recorded: Option<(i32, Kind, u32)>,
}

/// The instrument and its cable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuitarSound {
    pub sample_rate: f64,
    /// Peak the rendering is scaled to (what the input gain was set for), linear.
    pub peak: f32,
    /// Mains hum (fundamental and a few harmonics) and hiss, linear RMS against `peak` 1.
    pub hum: f32,
    pub hiss: f32,
    pub hum_hz: f64,
    /// Pickup position, fraction of the string from the bridge: a comb on the partials.
    pub pickup: f64,
    /// Stiffness: partial h at h·f0·√(1 + B h²).
    pub inharmonicity: f64,
}

impl Default for GuitarSound {
    fn default() -> Self {
        GuitarSound { sample_rate: 48_000.0, peak: 0.25, hum: 4e-4, hiss: 1.5e-4, hum_hz: 50.0, pickup: 0.12, inharmonicity: 8e-5 }
    }
}

/// Partials above this are left out: the detection low-passes far below.
const TOP_HZ: f64 = 5000.0;
/// A damped string fades with this time constant (s).
const RELEASE: f64 = 0.018;
/// Pitch and vibrato are updated every this many samples.
const BLOCK: usize = 32;

/// The voices played over `seconds`, as the cable carries them.
pub fn render(voices: &[Voice], seconds: f64, sound: &GuitarSound, seed: u64) -> Vec<f32> {
    render_with(voices, seconds, sound, seed, None)
}

/// `render`, each voice that has a recording in `samples` played from it (bends and slides,
/// which recordings cannot follow, and dead notes stay synthesised).
pub fn render_with(voices: &[Voice], seconds: f64, sound: &GuitarSound, seed: u64, samples: Option<&dyn Samples>) -> Vec<f32> {
    let sr = sound.sample_rate;
    let mut out = vec![0.0f32; (seconds * sr).ceil() as usize];
    let mut rng = Pcg32::new(seed);
    for v in voices {
        let recording = match (samples, v.recorded, v.excitation) {
            (_, _, Excitation::Dead { .. }) => None,
            (Some(bank), Some((fret, kind, choice)), _) if v.glide.is_empty() => bank.note(v.string, fret, kind, choice),
            _ => None,
        };
        match (recording, v.excitation) {
            (Some(rec), _) => play(v, rec, sound, &mut out),
            (None, Excitation::Dead { level }) => dead(v, level, sound, &mut out, &mut rng),
            (None, _) => ring(v, sound, &mut out, &mut rng),
        }
    }
    let loudest = out.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let scale = if loudest > 0.0 { sound.peak / loudest } else { 1.0 };
    let mut phase = 0.0f64;
    for x in out.iter_mut() {
        phase += TAU * sound.hum_hz / sr;
        let hum = phase.sin() + 0.5 * (2.0 * phase).sin() + 0.3 * (3.0 * phase).sin() + 0.15 * (5.0 * phase).sin();
        *x = *x * scale + sound.peak * (sound.hum * 0.8 * hum as f32 + sound.hiss * rng.gauss());
    }
    out
}

/// Pitch offset (semitones) `t` seconds after the attack.
fn offset(v: &Voice, t: f64) -> f64 {
    let glide = match v.glide.as_slice() {
        [] => 0.0,
        points => {
            let k = points.iter().position(|p| p.0 > t).unwrap_or(points.len());
            if k == 0 {
                points[0].1 * (t / points[0].0.max(1e-9)).clamp(0.0, 1.0)
            } else if k == points.len() {
                points[k - 1].1
            } else {
                let (a, b) = (points[k - 1], points[k]);
                a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0).max(1e-9)
            }
        }
    };
    glide + v.vibrato * (TAU * 5.5 * t).sin()
}

/// A ringing string: partials as damped phasors, their frequency following the glide.
fn ring(v: &Voice, sound: &GuitarSound, out: &mut [f32], rng: &mut Pcg32) {
    let sr = sound.sample_rate;
    let first = (v.start * sr).round().max(0.0) as usize;
    if first >= out.len() {
        return;
    }
    let f0 = 440.0 * 2f64.powf((v.midi - 69.0) / 12.0);
    let (level, pos, tilt, click, node) = match v.excitation {
        Excitation::Pick { level, pos } => (level as f64, pos as f64, 1.0, 1.0, 1),
        Excitation::Legato { level } => (level as f64, 0.5, 1.6, 0.0, 1),
        Excitation::Harmonic { level, n } => (level as f64, 0.0, 1.2, 0.4, n.max(1)),
        Excitation::Dead { .. } => return,
    };
    // Low strings ring longer; a palm mute damps fast, the highs first.
    let t60 = if v.palm { 0.22 } else { (4.0 * (82.4 / f0).sqrt()).clamp(1.2, 6.0) };
    let tilt = if v.palm { tilt + 0.8 } else { tilt };
    let mut partials: Vec<(f64, f64, f64)> = Vec::new(); // (amplitude, frequency ratio, decay per sample)
    let mut norm = 0.0;
    for h in (node..).step_by(node as usize) {
        let ratio = h as f64 * (1.0 + sound.inharmonicity * (h * h) as f64).sqrt();
        if f0 * ratio >= TOP_HZ.min(0.45 * sr) {
            break;
        }
        let hf = h as f64;
        let excite = if node > 1 { 1.0 } else { (PI * hf * pos).sin().abs() };
        let a = excite / hf.powf(tilt) * (PI * hf * sound.pickup).sin().abs();
        if a < 1e-4 {
            continue;
        }
        let spread = if v.palm { 0.3 } else { 0.04 };
        let decay = (-(1000f64.ln()) / (t60 / (1.0 + spread * (hf - 1.0).powi(2)) * sr)).exp();
        partials.push((a, ratio, decay));
        norm += a;
    }
    if partials.is_empty() {
        return;
    }
    let scale = level / norm;
    let stop = (((v.end - v.start).max(0.0) + 6.0 * RELEASE + 0.01) * sr) as usize;
    let len = stop.min(out.len() - first).min((t60 * 1.5 * sr) as usize);
    let damp_at = ((v.end - v.start).max(0.0) * sr) as usize;
    let damp = (-1.0 / (RELEASE * sr)).exp();
    let attack = (0.0008 * sr) as usize;
    // Each partial: z ← z·r·e^{iω}, the sound is Im(z); ω follows the pitch block by block.
    let mut z: Vec<(f64, f64)> = partials.iter().map(|p| (p.0 * scale, 0.0)).collect();
    let mut env = 1.0f64;
    let mut i = 0;
    while i < len {
        let n = BLOCK.min(len - i);
        let t = (i + n / 2) as f64 / sr;
        let f = f0 * 2f64.powf(offset(v, t) / 12.0);
        let rot: Vec<(f64, f64)> = partials
            .iter()
            .map(|&(_, ratio, decay)| {
                let w = TAU * f * ratio / sr;
                (decay * w.cos(), decay * w.sin())
            })
            .collect();
        for k in 0..n {
            let j = i + k;
            if j >= damp_at {
                env *= damp;
            }
            let ramp = if j < attack { j as f64 / attack as f64 } else { 1.0 };
            let mut sum = 0.0;
            for (zz, &(c, s)) in z.iter_mut().zip(&rot) {
                *zz = (zz.0 * c - zz.1 * s, zz.0 * s + zz.1 * c);
                sum += zz.1;
            }
            out[first + j] += (sum * env * ramp) as f32;
        }
        i += n;
        if env < 1e-4 {
            break;
        }
    }
    // The pick's click: a fast-decaying, differentiated noise burst.
    if click > 0.0 {
        let g = (0.25 * click * level) as f32;
        let tau = 0.0015 * sr;
        let mut prev = 0.0;
        for k in 0..((0.006 * sr) as usize).min(out.len() - first) {
            let e = g * rng.gauss() * (-(k as f64) / tau).exp() as f32;
            out[first + k] += 0.5 * (e - prev);
            prev = e;
        }
    }
}

/// A recorded note from the voice's start, at its level, damped at its end. A hammer-on or
/// pull-off leaves out the pick's attack.
fn play(v: &Voice, rec: &[f32], sound: &GuitarSound, out: &mut [f32]) {
    let sr = sound.sample_rate;
    let first = (v.start * sr).round().max(0.0) as usize;
    let (level, skip) = match v.excitation {
        Excitation::Pick { level, .. } | Excitation::Harmonic { level, .. } | Excitation::Dead { level } => (level, 0),
        Excitation::Legato { level } => (level, (0.008 * sr) as usize),
    };
    let damp_at = ((v.end - v.start).max(0.0) * sr) as usize;
    let damp = (-1.0 / (RELEASE * sr)).exp() as f32;
    let fade_in = if skip > 0 { (0.003 * sr) as usize } else { 0 };
    let mut env = 1.0f32;
    for (k, &x) in rec.iter().skip(skip).enumerate() {
        let j = first + k;
        if j >= out.len() {
            break;
        }
        if k >= damp_at {
            env *= damp;
            if env < 1e-4 {
                break;
            }
        }
        let ramp = if k < fade_in { k as f32 / fade_in as f32 } else { 1.0 };
        out[j] += x * level * env * ramp;
    }
}

/// A dead note: a noisy thump, with a trace of the string's pitch, gone in a few tens of ms.
fn dead(v: &Voice, level: f32, sound: &GuitarSound, out: &mut [f32], rng: &mut Pcg32) {
    let sr = sound.sample_rate;
    let first = (v.start * sr).round().max(0.0) as usize;
    let len = ((0.06 * sr) as usize).min(out.len().saturating_sub(first));
    let f0 = 440.0 * 2f64.powf((v.midi - 69.0) / 12.0);
    let (tau, low) = (0.008 * sr, (-TAU * 1500.0 / sr).exp());
    let mut lp = 0.0f64;
    for k in 0..len {
        let t = k as f64;
        lp = low * lp + (1.0 - low) * rng.gauss() as f64;
        let thump = 0.3 * (TAU * f0 * t / sr).sin() * (-t / (0.025 * sr)).exp();
        out[first + k] += (level as f64 * (0.6 * lp * (-t / tau).exp() + thump)) as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::{Mpm, PitchConfig, PitchDetector};

    fn voice(midi: f64, start: f64, end: f64) -> Voice {
        Voice { string: 0, start, end, midi, glide: Vec::new(), vibrato: 0.0, excitation: Excitation::Pick { level: 1.0, pos: 0.15 }, palm: false, recorded: None }
    }

    /// Pitch (MIDI) of `x` around `at` seconds.
    fn pitch_at(x: &[f32], at: f64) -> f64 {
        let mut mpm = Mpm::new(PitchConfig::default());
        let i = (at * 48_000.0) as usize;
        let est = mpm.detect(&x[i..i + 2048]).expect("a pitch");
        69.0 + 12.0 * (est.freq_hz as f64 / 440.0).log2()
    }

    #[test]
    fn a_picked_note_sounds_its_pitch_and_stops_when_damped() {
        let x = render(&[voice(45.0, 0.1, 0.6)], 1.0, &GuitarSound { hum: 0.0, hiss: 0.0, ..GuitarSound::default() }, 1);
        assert!((pitch_at(&x, 0.3) - 45.0).abs() < 0.1);
        let rms = |a: f64, b: f64| {
            let s = &x[(a * 48_000.0) as usize..(b * 48_000.0) as usize];
            (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
        };
        assert!(rms(0.2, 0.3) > 100.0 * rms(0.75, 0.85), "damped at 0.6 s");
        let peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.25).abs() < 1e-6, "scaled to the cable's level");
    }

    #[test]
    fn a_bend_and_a_harmonic_sound_where_they_should() {
        let mut bend = voice(57.0, 0.0, 1.0);
        bend.glide = vec![(0.08, 0.0), (0.2, 2.0)]; // picked, held, then bent
        let x = render(&[bend], 1.0, &GuitarSound { hum: 0.0, hiss: 0.0, ..GuitarSound::default() }, 2);
        assert!((pitch_at(&x, 0.02) - 57.0).abs() < 0.3, "picked at the fret");
        assert!((pitch_at(&x, 0.5) - 59.0).abs() < 0.15, "a whole step up");
        let mut harmonic = voice(40.0, 0.0, 1.5);
        harmonic.excitation = Excitation::Harmonic { level: 1.0, n: 3 };
        let x = render(&[harmonic], 1.5, &GuitarSound { hum: 0.0, hiss: 0.0, ..GuitarSound::default() }, 3);
        assert!((pitch_at(&x, 0.4) - 59.0).abs() < 0.15, "7th-fret harmonic: an octave and a fifth up");
    }

    #[test]
    fn a_dead_note_is_short() {
        let mut v = voice(45.0, 0.1, 1.0);
        v.excitation = Excitation::Dead { level: 1.0 };
        let x = render(&[v, voice(64.0, 0.5, 0.9)], 1.0, &GuitarSound { hum: 0.0, hiss: 0.0, ..GuitarSound::default() }, 4);
        let energy = |a: f64, b: f64| x[(a * 48_000.0) as usize..(b * 48_000.0) as usize].iter().map(|v| v * v).sum::<f32>();
        assert!(energy(0.1, 0.16) > 0.0 && energy(0.25, 0.45) < 1e-9, "silent long before the next note");
    }
}
