//! Pitch accuracy of MPM and YIN on every string × fret, clean, noisy, detuned and quiet.

mod common;

use common::*;
use opensmith_dsp::fretboard::{FretPos, Tuning, MAX_FRET, NUM_STRINGS};
use opensmith_dsp::synth::{white_noise, Pcg32};
use opensmith_dsp::{db_to_lin, note, Mpm, PitchConfig, PitchDetector, Yin};

fn detectors() -> Vec<(&'static str, Box<dyn PitchDetector>)> {
    let cfg = PitchConfig::default();
    vec![("MPM", Box::new(Mpm::new(cfg))), ("YIN", Box::new(Yin::new(cfg)))]
}

#[test]
fn open_strings_pure_sines_note_string_and_cents() {
    for (algo, mut det) in detectors() {
        for (string, (label, f, midi)) in OPEN_STRINGS.into_iter().enumerate() {
            let x = common::render(Kind::Sine, f, 0.1, 0.5, 0);
            let e = det.detect(&x[..2048]).unwrap_or_else(|| panic!("{algo}: no pitch for {label}"));
            let (m, _) = note::nearest_note(e.freq_hz, note::A4_HZ);
            assert_eq!(m, midi, "{algo} {label}");
            assert_eq!(Tuning::STANDARD.resolve(m, 0), Some(FretPos::new(string as u8, 0)), "{algo} {label}");
            let err = note::cents_between(e.freq_hz, f);
            assert!(err.abs() < 0.5, "{algo} {label}: {err:+.3} cents");
            assert!(e.clarity > 0.99, "{algo} {label}: clarity {}", e.clarity);
        }
    }
}

/// 6 strings × 25 frets × {additive pluck, Karplus–Strong} = 300 notes per algorithm.
#[test]
fn every_string_every_fret_plucked_is_100_percent() {
    let tuning = Tuning::STANDARD;
    for (algo, mut det) in detectors() {
        let (mut total, mut worst) = (0, 0f32);
        let mut failures = Vec::new();
        for string in 0..NUM_STRINGS {
            for fret in 0..=MAX_FRET {
                let midi = tuning.midi_at(FretPos::new(string as u8, fret));
                let f = midi_freq(midi);
                for kind in [Kind::Pluck, Kind::Ks] {
                    let seed = (string * 1000 + fret as usize * 10) as u64 + kind as u64;
                    let x = common::render(kind, f, 0.15, 0.4, seed);
                    let at = (0.05 * SR) as usize;
                    total += 1;
                    match det.detect(&x[at..at + 2048]) {
                        Some(e) if note::nearest_note(e.freq_hz, note::A4_HZ).0 == midi => {
                            worst = worst.max(note::cents_between(e.freq_hz, f).abs());
                        }
                        other => failures.push(format!("{} s{string} f{fret} {:?} → {other:?}", note::note_name(midi), kind)),
                    }
                }
            }
        }
        println!("{algo}: {}/{total} correct, worst deviation {worst:.2} cents", total - failures.len());
        assert!(failures.is_empty(), "{algo} failures:\n{}", failures.join("\n"));
        assert!(worst < 10.0, "{algo}: worst deviation {worst} cents");
    }
}

/// White noise at 20 dB SNR, strings detuned by up to ±30 cents, levels from −50 to −6 dBFS.
#[test]
fn noisy_detuned_and_quiet_notes_stay_100_percent() {
    let tuning = Tuning::STANDARD;
    let mut rng = Pcg32::new(42);
    for (algo, mut det) in detectors() {
        let (mut total, mut failures) = (0, Vec::new());
        for string in 0..NUM_STRINGS {
            for fret in (0..=MAX_FRET).step_by(3) {
                let midi = tuning.midi_at(FretPos::new(string as u8, fret));
                let detune = rng.range(-30.0, 30.0);
                let f = midi_freq(midi) * (detune / 1200.0).exp2();
                let level_db = rng.range(-50.0, -6.0);
                let mut x = common::render(Kind::Pluck, f, 0.15, db_to_lin(level_db), rng.next_u32() as u64);
                let at = (0.05 * SR) as usize;
                let rms = (x[at..at + 2048].iter().map(|v| v * v).sum::<f32>() / 2048.0).sqrt();
                let noise = white_noise(&mut rng, x.len(), rms * 0.1);
                x.iter_mut().zip(&noise).for_each(|(v, n)| *v += n);
                total += 1;
                match det.detect(&x[at..at + 2048]) {
                    Some(e) => {
                        let (m, cents) = note::nearest_note(e.freq_hz, note::A4_HZ);
                        if m != midi || (cents - detune).abs() > 10.0 {
                            failures.push(format!("{} detune {detune:+.1}c @ {level_db:.0} dBFS → {m} {cents:+.1}c", note::note_name(midi)));
                        }
                    }
                    None => failures.push(format!("{} detune {detune:+.1}c @ {level_db:.0} dBFS → none", note::note_name(midi))),
                }
            }
        }
        println!("{algo}: {}/{total} correct under noise/detune/level", total - failures.len());
        assert!(failures.is_empty(), "{algo} failures:\n{}", failures.join("\n"));
    }
}

#[test]
fn noise_alone_never_yields_a_confident_pitch() {
    let mut rng = Pcg32::new(7);
    let x = white_noise(&mut rng, 48_000, 0.1);
    for (algo, mut det) in detectors() {
        for w in x.chunks_exact(2048) {
            if let Some(e) = det.detect(w) {
                assert!(e.clarity < 0.9, "{algo}: noise reported {e:?}");
            }
        }
        assert!(det.detect(&[0.0; 2048]).is_none(), "{algo}: silence must be unvoiced");
    }
}
