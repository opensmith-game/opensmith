//! Polyphonic chord verification (FFT peaks + harmonic explanation + chromagram).

mod common;

use common::*;
use opensmith_dsp::note::{self, parse_note, pitch_class};
use opensmith_dsp::synth::{mix_into, strum, Pcg32, Pluck};
use opensmith_dsp::{ChordConfig, ChordVerifier};

const STRUM_MS: f32 = 12.0;

/// Common open and barre voicings, low string first.
const CHORDS: &[(&str, &[&str])] = &[
    ("E5", &["E2", "B2", "E3"]),
    ("A5", &["A2", "E3", "A3"]),
    ("E", &["E2", "B2", "E3", "G#3", "B3", "E4"]),
    ("Em", &["E2", "B2", "E3", "G3", "B3", "E4"]),
    ("A", &["A2", "E3", "A3", "C#4", "E4"]),
    ("Am", &["A2", "E3", "A3", "C4", "E4"]),
    ("D", &["D3", "A3", "D4", "F#4"]),
    ("Dm", &["D3", "A3", "D4", "F4"]),
    ("G", &["G2", "B2", "D3", "G3", "B3", "G4"]),
    ("C", &["C3", "E3", "G3", "C4", "E4"]),
    ("F", &["F2", "C3", "F3", "A3", "C4", "F4"]),
    ("B7", &["B2", "D#3", "A3", "B3", "F#4"]),
];

fn midis(names: &[&str]) -> Vec<i32> {
    names.iter().map(|n| parse_note(n).unwrap()).collect()
}

fn pitch_classes(m: &[i32]) -> u16 {
    m.iter().fold(0, |acc, &n| acc | 1 << pitch_class(n))
}

/// Strummed chord: one pluck per string, random pluck position/tilt per string.
fn play(m: &[i32], seed: u64) -> Vec<f32> {
    let mut rng = Pcg32::new(seed);
    let plucks: Vec<Pluck> = m
        .iter()
        .map(|&n| {
            let mut p = Pluck::new(midi_freq(n));
            p.secs = 0.5;
            p.amp = rng.range(0.15, 0.3);
            p.pluck_pos = rng.range(0.1, 0.3);
            p.tilt = rng.range(1.0, 1.6);
            p.pick_noise = 0.2;
            p
        })
        .collect();
    strum(&plucks, SR, STRUM_MS, &mut rng)
}

/// Window ending `after_ms` after the last string's attack.
fn window(x: &[f32], n_strings: usize, after_ms: f32) -> &[f32] {
    let end = (((n_strings - 1) as f32 * STRUM_MS + after_ms) * 1e-3 * SR) as usize;
    &x[..end]
}

#[test]
fn every_chord_is_accepted_when_played() {
    let mut v = ChordVerifier::new(ChordConfig::default());
    for (seed, &(name, notes)) in CHORDS.iter().enumerate() {
        let m = midis(notes);
        let x = play(&m, seed as u64);
        let r = v.verify(window(&x, m.len(), 100.0), &m, note::A4_HZ);
        assert!(r.accepted, "{name}: {}/{} notes, foreign {:.3}, present {:?}", r.n_present(), m.len(), r.foreign_ratio, &r.present[..m.len()]);
    }
}

/// Any chord missing a pitch class of the expected chord must be rejected (132 pairs, of
/// which those where the played chord holds every expected pitch class are skipped).
#[test]
fn chords_missing_an_expected_pitch_class_are_rejected() {
    let mut v = ChordVerifier::new(ChordConfig::default());
    let (mut checked, mut failures) = (0, Vec::new());
    for (i, &(played, pn)) in CHORDS.iter().enumerate() {
        let pm = midis(pn);
        let x = play(&pm, 100 + i as u64);
        let w = window(&x, pm.len(), 100.0);
        for &(expected, en) in CHORDS {
            let em = midis(en);
            if pitch_classes(&em) & !pitch_classes(&pm) == 0 {
                continue;
            }
            checked += 1;
            let r = v.verify(w, &em, note::A4_HZ);
            if r.accepted {
                failures.push(format!("played {played}, expected {expected}: accepted"));
            }
        }
    }
    println!("{checked} wrong-chord pairs rejected");
    assert!(checked > 100);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn major_minor_and_missing_string_are_told_apart() {
    let mut v = ChordVerifier::new(ChordConfig::default());
    let e = midis(&["E2", "B2", "E3", "G#3", "B3", "E4"]);
    let em = midis(&["E2", "B2", "E3", "G3", "B3", "E4"]);
    let x = play(&e, 7);
    assert!(v.verify(window(&x, 6, 100.0), &e, note::A4_HZ).accepted);
    assert!(!v.verify(window(&x, 6, 100.0), &em, note::A4_HZ).accepted, "E major accepted as E minor");
    let y = play(&em, 8);
    assert!(!v.verify(window(&y, 6, 100.0), &e, note::A4_HZ).accepted, "E minor accepted as E major");
    // G#3 string not sounding: E major is incomplete.
    let without = midis(&["E2", "B2", "E3", "B3", "E4"]);
    let z = play(&without, 9);
    let r = v.verify(window(&z, 5, 100.0), &e, note::A4_HZ);
    assert!(!r.accepted && !r.present[3], "missing G#3 not detected: {r:?}");
}

#[test]
fn a_dissonant_extra_note_is_rejected() {
    let mut v = ChordVerifier::new(ChordConfig::default());
    let e5 = midis(&["E2", "B2", "E3"]);
    let mut x = play(&e5, 3);
    let mut rng = Pcg32::new(4);
    let mut f3 = Pluck::new(midi_freq(parse_note("F3").unwrap()));
    f3.amp = 0.25;
    f3.secs = 0.5;
    mix_into(&mut x, &f3.render(SR, &mut rng), (2.0 * STRUM_MS * 1e-3 * SR) as usize, 1.0);
    let r = v.verify(window(&x, 3, 100.0), &e5, note::A4_HZ);
    assert!(!r.accepted, "E5 + F3 accepted as E5 (foreign {:.3}, {:.1} dB)", r.foreign_ratio, r.foreign_peak_db);
    assert!(r.foreign_peak_db > ChordConfig::default().foreign_peak_db, "{r:?}");
}

#[test]
fn chromagram_concentrates_on_the_chord_tones() {
    let mut v = ChordVerifier::new(ChordConfig::default());
    for (seed, &(name, notes)) in CHORDS.iter().enumerate() {
        let m = midis(notes);
        let x = play(&m, 200 + seed as u64);
        v.analyze(window(&x, m.len(), 100.0));
        let chroma = v.chroma(note::A4_HZ);
        let pcs = pitch_classes(&m);
        let total: f32 = chroma.iter().sum();
        let inside: f32 = (0..12).filter(|pc| pcs >> pc & 1 == 1).map(|pc| chroma[pc]).sum();
        assert!(inside / total > 0.75, "{name}: only {:.0}% of chroma on chord tones ({chroma:?})", 100.0 * inside / total);
    }
}

#[test]
fn silence_and_noise_are_never_a_chord() {
    let mut v = ChordVerifier::new(ChordConfig::default());
    let e = midis(&["E2", "B2", "E3", "G#3", "B3", "E4"]);
    assert!(!v.verify(&[0.0; 4096], &e, note::A4_HZ).accepted);
    let noise = opensmith_dsp::synth::white_noise(&mut Pcg32::new(5), 4096, 0.1);
    assert!(!v.verify(&noise, &e, note::A4_HZ).accepted);
}

/// Earliest acceptance after the last string's attack, by window length: the analysis window
/// grows from the first string's attack (capped at `window`).
#[test]
fn chord_verification_latency() {
    let cfg = ChordConfig::default();
    let mut v = ChordVerifier::new(cfg);
    let mut worst = 0f32;
    println!("\nchord  accepted after last string (ms)");
    for (seed, &(name, notes)) in CHORDS.iter().enumerate() {
        let m = midis(notes);
        let x = play(&m, 300 + seed as u64);
        let last = ((m.len() - 1) as f32 * STRUM_MS * 1e-3 * SR) as usize;
        let accepted_at = (1..)
            .map(|k| last + k * BLOCK)
            .take_while(|&end| end <= x.len())
            .find(|&end| {
                let r = v.verify(&x[..end], &m, note::A4_HZ);
                r.accepted
            })
            .map(|end| ms((end - last) as u64));
        let t = accepted_at.unwrap_or_else(|| panic!("{name} never accepted"));
        println!("{name:5}  {t:6.2}");
        worst = worst.max(t);
    }
    println!("worst {worst:.2} ms");
    assert!(worst <= 100.0, "chord verification needs {worst} ms");
}
