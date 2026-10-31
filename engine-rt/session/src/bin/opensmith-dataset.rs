//! Training and test data for the learned note detector (`opensmith_dsp::notefeat`): the
//! features of guitar playing, every 10 ms, with the notes attacked in it.
//!
//! cargo run --release -p opensmith-session --bin opensmith-dataset -- --bank <takes of single notes> --out <folder> [--pieces N] [--seconds S] [--seed N]
//!     pieces invented by the composer, played by the virtual guitarist on recorded notes;
//! cargo run --release -p opensmith-session --bin opensmith-dataset -- --takes <takes> --out <folder>
//!     real playing whose chart says what was played.
//!
//! Each item is `<name>.f32` (frames × `BINS` little-endian floats) and `<name>.json` (the
//! notes: start and end in seconds, sounding pitch in MIDI, how it was played).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use opensmith_dsp::guitar::{Excitation, Voice};
use opensmith_dsp::notefeat::{NoteFeatures, BINS, HOP, SAMPLE_RATE};
use opensmith_dsp::synth::Pcg32;
use opensmith_session::composer::compose;
use opensmith_session::guitarist::{perform_with, Player};
use opensmith_session::samples::SampleBank;
use opensmith_session::Take;
use serde_json::json;

/// Sounding pitch of a voice at its attack (a harmonic sounds its partial).
fn sounding(v: &Voice) -> f64 {
    match v.excitation {
        Excitation::Harmonic { n, .. } => v.midi + 12.0 * (n as f64).log2(),
        _ => v.midi,
    }
}

fn kind(v: &Voice) -> &'static str {
    match v.excitation {
        Excitation::Pick { .. } if v.palm => "palm",
        Excitation::Pick { .. } => "pick",
        Excitation::Legato { .. } => "legato",
        Excitation::Dead { .. } => "dead",
        Excitation::Harmonic { .. } => "harmonic",
    }
}

fn write(out: &Path, name: &str, guitar: &[f32], notes: Vec<serde_json::Value>) {
    let mut nf = NoteFeatures::new();
    let feats = nf.spectrogram(guitar);
    let bytes: Vec<u8> = feats.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(out.join(format!("{name}.f32")), bytes).expect("features written");
    let doc = json!({ "frames": feats.len() / BINS, "bins": BINS, "hop": HOP, "sampleRate": SAMPLE_RATE, "notes": notes });
    std::fs::write(out.join(format!("{name}.json")), doc.to_string()).expect("notes written");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut bank, mut takes, mut out, mut pieces, mut seconds, mut seed) = (None, None, None, 100usize, 60.0f64, 1u64);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match a.as_str() {
            "--bank" => bank = Some(PathBuf::from(value())),
            "--takes" => takes = Some(PathBuf::from(value())),
            "--out" => out = Some(PathBuf::from(value())),
            "--pieces" => pieces = value().parse().unwrap_or(100),
            "--seconds" => seconds = value().parse().unwrap_or(60.0),
            "--seed" => seed = value().parse().unwrap_or(1),
            _ => {}
        }
    }
    let Some(out) = out else {
        eprintln!("usage : opensmith-dataset (--bank <prises de notes> | --takes <prises>) --out <dossier> [--pieces N] [--seconds S] [--seed N]");
        std::process::exit(2);
    };
    std::fs::create_dir_all(&out).expect("output folder");
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).saturating_sub(3).max(1);

    if let Some(dir) = takes {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir).expect("takes folder").flatten().map(|e| e.path()).filter(|p| p.join("take.json").is_file()).collect();
        paths.sort();
        for path in paths {
            let Ok(take) = Take::load(&path) else { continue };
            let c = &take.chart;
            let t0 = take.anchors.first().map_or(0.0, |a| a.1);
            let mut notes = Vec::new();
            for n in &c.notes {
                let tech = n.techniques.clone().unwrap_or_default();
                let mut pitch = c.pitch(n.string, n.fret) as f64;
                if tech.harmonic == Some(true) {
                    pitch = c.pitch(n.string, 0) as f64 + match n.fret { 12 => 12.0, 7 | 19 => 19.02, 5 | 24 => 24.0, 4 | 9 | 16 => 27.86, _ => 31.02 };
                }
                let k = if tech.palm_mute == Some(true) { "palm" } else if tech.harmonic == Some(true) { "harmonic" } else { "pick" };
                notes.push(json!({ "start": n.time - t0, "end": n.time - t0 + n.sustain.max(0.1), "pitch": pitch, "kind": k }));
            }
            for ch in &c.chords {
                for m in c.chord_pitches(ch.template) {
                    notes.push(json!({ "start": ch.time - t0, "end": ch.time - t0 + ch.sustain.max(0.1), "pitch": m as f64, "kind": "pick" }));
                }
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            write(&out, &name, &take.guitar, notes);
            println!("{name}");
        }
        return;
    }

    let bank = SampleBank::from_takes(&bank.expect("--bank or --takes")).expect("recorded notes");
    let next = Mutex::new(0usize);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let k = {
                    let mut n = next.lock().unwrap();
                    *n += 1;
                    *n - 1
                };
                if k >= pieces {
                    break;
                }
                let s = seed.wrapping_mul(1_000_003).wrapping_add(k as u64);
                let chart = compose(s, seconds);
                let player = [Player::clean(), Player::good(), Player::beginner()][k % 3].clone();
                let perf = perform_with(&chart, u32::MAX, &player, s, Some(&bank));
                // The input gain the player set, from cautious to hot.
                let mut rng = Pcg32::new(s ^ 0x5bd1_e995);
                let gain = rng.range(0.3, 1.6);
                let guitar: Vec<f32> = perf.take.guitar.iter().map(|v| (v * gain).clamp(-1.0, 1.0)).collect();
                let notes: Vec<serde_json::Value> =
                    perf.voices.iter().map(|v| json!({ "start": v.start, "end": v.end, "pitch": sounding(v), "kind": kind(v) })).collect();
                write(&out, &format!("piece_{k:06}"), &guitar, notes);
            });
        }
    });
    println!("{pieces} morceaux écrits dans {}", out.display());
}
