//! `opensmith-diag` — Engine-RT diagnostics: CoreAudio devices, live duplex run with note
//! detection, synthetic self-test, reference WAV generation and offline rendering.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use opensmith_audio::{list_devices, Engine, EngineConfig, InputKind, ERROR_KINDS};
use opensmith_dsp::fretboard::{STRING_COLORS, STRING_NAMES};
use opensmith_dsp::fx::cabinet::CabinetIr;
use opensmith_dsp::fx::drive::DriveParams;
use opensmith_dsp::fx::gate::GateParams;
use opensmith_dsp::fx::resample::resample;
use opensmith_dsp::synth::{mix_into, sine, strum, Pcg32, Pluck};
use opensmith_dsp::{
    lin_to_db, note, wav, Algorithm, ChainParams, ChordConfig, ChordVerifier, GuitarChain, Mpm, NoteTracker, PitchDetector,
    TrackerConfig, TrackerEvent, Tuning,
};

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

#[derive(Parser)]
#[command(name = "opensmith-diag", version, about = "OpenSmith Engine-RT diagnostics")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List CoreAudio devices and flag the Real Tone Cable.
    Devices,
    /// Open the duplex stream (guitar → note tracker + amp → output) and print notes live.
    Run(RunArgs),
    /// Synthetic end-to-end check: per-string latency, chords, CPU cost. Fails on regression.
    Selftest,
    /// Write reference WAVs (open strings, chords, riff) into DIR.
    Gen {
        #[arg(default_value = "out/testdata")]
        dir: PathBuf,
    },
    /// Offline: run a WAV through the note tracker and the amp chain.
    Render {
        input: PathBuf,
        output: PathBuf,
        #[command(flatten)]
        tone: ToneArgs,
    },
}

#[derive(Args, Clone)]
struct ToneArgs {
    /// Overdrive amount, 0..1.
    #[arg(long, default_value_t = 0.5)]
    drive: f32,
    /// Tone, 0 (dark) ..1 (bright).
    #[arg(long, default_value_t = 0.5)]
    tone: f32,
    /// Bypass the overdrive.
    #[arg(long)]
    clean: bool,
    /// Noise-gate threshold (dBFS).
    #[arg(long, default_value_t = -58.0, allow_hyphen_values = true)]
    gate: f32,
    #[arg(long)]
    no_gate: bool,
    /// Cabinet impulse response (.wav); default: built-in 4x12.
    #[arg(long)]
    ir: Option<PathBuf>,
    #[arg(long)]
    no_cab: bool,
}

impl ToneArgs {
    fn chain(&self) -> ChainParams {
        ChainParams {
            gate: GateParams { enabled: !self.no_gate, threshold_db: self.gate, ..GateParams::default() },
            drive: DriveParams { enabled: !self.clean, drive: self.drive, tone: self.tone, ..DriveParams::default() },
            cab_enabled: !self.no_cab,
            ..ChainParams::default()
        }
    }

    fn ir(&self) -> Result<Option<CabinetIr>> {
        self.ir
            .as_ref()
            .map(|p| CabinetIr::from_wav(p, SR, 500.0).with_context(|| format!("loading IR {}", p.display())))
            .transpose()
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum Algo {
    Mpm,
    Yin,
}

#[derive(Args)]
struct RunArgs {
    /// Input device name fragment (default: USB guitar cable, else system input).
    #[arg(long)]
    input: Option<String>,
    /// Output device name fragment (default: system output).
    #[arg(long)]
    output: Option<String>,
    #[arg(long, default_value_t = 48_000)]
    rate: u32,
    /// CoreAudio buffer in frames.
    #[arg(long, default_value_t = 64)]
    buffer: u32,
    /// Seconds to run (0 = until Ctrl-C).
    #[arg(long, default_value_t = 0.0)]
    seconds: f32,
    /// Monitor even without the USB guitar cable (feedback risk with a built-in mic).
    #[arg(long, conflicts_with = "no_monitor")]
    monitor: bool,
    #[arg(long)]
    no_monitor: bool,
    /// Monitor volume (dB).
    #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
    volume_db: f32,
    #[arg(long, value_enum, default_value_t = Algo::Mpm)]
    algo: Algo,
    /// Concert pitch.
    #[arg(long, default_value_t = 440.0)]
    a4: f32,
    #[command(flatten)]
    tone: ToneArgs,
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Devices => devices(),
        Cmd::Run(args) => run(args),
        Cmd::Selftest => selftest(),
        Cmd::Gen { dir } => generate(&dir),
        Cmd::Render { input, output, tone } => render(&input, &output, &tone),
    }
}

fn colored(string: usize, text: &str) -> String {
    let [r, g, b] = STRING_COLORS[string].rgb;
    format!("\x1b[1;38;2;{r};{g};{b}m{text}\x1b[0m")
}

fn rates(ranges: &[(u32, u32)]) -> String {
    let mut v: Vec<String> = ranges.iter().map(|&(a, b)| if a == b { a.to_string() } else { format!("{a}-{b}") }).collect();
    v.dedup();
    v.join(",")
}

fn devices() -> Result<()> {
    let list = list_devices()?;
    println!("{:<34} {:>3} {:>3}  {:<10} {:<22} {:<10} flags", "device", "in", "out", "interface", "rates (Hz)", "buffer");
    for d in &list {
        let mut flags = Vec::new();
        if d.rocksmith {
            flags.push("GUITAR CABLE");
        }
        if d.default_input {
            flags.push("default-in");
        }
        if d.default_output {
            flags.push("default-out");
        }
        let all_rates = [d.input_rates.clone(), d.output_rates.clone()].concat();
        let buffer = d.buffer_frames.map_or("?".into(), |(a, b)| format!("{a}-{b}"));
        let ok48 = d.supports_rate(48_000, d.input_channels > 0) || d.supports_rate(48_000, false);
        println!(
            "{:<34} {:>3} {:>3}  {:<10} {:<22} {:<10} {}{}",
            d.name,
            d.input_channels,
            d.output_channels,
            d.interface,
            rates(&all_rates),
            buffer,
            flags.join(" "),
            if ok48 { "" } else { " (no 48 kHz)" }
        );
    }
    match list.iter().find(|d| d.rocksmith) {
        Some(d) => println!("\n✔ Real Tone Cable found: \"{}\"", d.name),
        None => println!(
            "\n✘ Real Tone Cable not connected — `run` will fall back to the default input ({}).",
            list.iter().find(|d| d.default_input).map_or("none", |d| d.name.as_str())
        ),
    }
    Ok(())
}

fn run(args: RunArgs) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || r.store(false, Ordering::SeqCst)).context("installing Ctrl-C handler")?;

    let mut tracker = TrackerConfig { a4_hz: args.a4, ..TrackerConfig::default() };
    tracker.algorithm = match args.algo {
        Algo::Mpm => Algorithm::Mpm,
        Algo::Yin => Algorithm::Yin,
    };
    let cfg = EngineConfig {
        input: args.input.clone(),
        output: args.output.clone(),
        sample_rate: args.rate,
        buffer_frames: args.buffer,
        monitor: if args.monitor { Some(true) } else if args.no_monitor { Some(false) } else { None },
        monitor_gain_db: args.volume_db,
        chain: args.tone.chain(),
        ir: args.tone.ir()?,
        tracker,
        ..EngineConfig::default()
    };
    let mut engine = Engine::start(cfg)?;
    let info = engine.info().clone();
    let kind = match info.input_kind {
        InputKind::Rocksmith => "Real Tone Cable",
        InputKind::Requested => "requested",
        InputKind::DefaultFallback => "FALLBACK: cable not found, system default input",
    };
    println!("input   {} ({kind}), {} ch", info.input_name, info.input_channels);
    println!("output  {}, {} ch", info.output_name, info.output_channels);
    println!(
        "stream  {} Hz, buffer {} frames requested (input {}, output {})",
        info.sample_rate,
        info.requested_buffer,
        if info.input_fixed { "fixed" } else { "device default" },
        if info.output_fixed { "fixed" } else { "device default" }
    );
    if info.monitor {
        println!("monitor ON");
    } else {
        println!("monitor OFF (no USB guitar cable: avoiding microphone → speaker feedback; --monitor to force)");
    }
    println!("Ctrl-C to stop.\n");

    let sr = info.sample_rate as f32;
    let tuning = Tuning::STANDARD;
    let mut hand = 0u8;
    let started = Instant::now();
    let mut last_report = Instant::now();
    let mut notes = 0usize;
    let mut last_pitch: Option<(f32, f32)> = None;
    while running.load(Ordering::SeqCst) && (args.seconds <= 0.0 || started.elapsed().as_secs_f32() < args.seconds) {
        engine.poll_events(|e| match e {
            TrackerEvent::NoteOn { onset_sample, sample, midi, freq_hz, cents, clarity, legato } => {
                notes += 1;
                let name = note::note_name(midi);
                let pos = tuning.resolve(midi, hand);
                let place = match pos {
                    Some(p) => {
                        hand = p.fret;
                        colored(p.string as usize, &format!("{} string, fret {:>2}", STRING_NAMES[p.string as usize], p.fret))
                    }
                    None => "off the fretboard".into(),
                };
                println!(
                    "♪ {name:<4} {place}  {freq_hz:7.2} Hz {cents:+5.1}¢  clarity {clarity:.2}  confirmed {:5.2} ms after attack{}",
                    (sample - onset_sample) as f32 * 1000.0 / sr,
                    if legato { "  (legato)" } else { "" }
                );
            }
            TrackerEvent::Pitch { midi_f, clarity, .. } => last_pitch = Some((midi_f, clarity)),
            _ => {}
        });
        if last_report.elapsed() >= Duration::from_secs(1) {
            let s = engine.stats();
            let budget_us = s.in_block.max(1) as f64 / sr as f64 * 1e6;
            let tuner = last_pitch.take().map_or(String::new(), |(m, _)| {
                let n = m.round() as i32;
                format!(" | {} {:+.1}¢", note::note_name(n), (m - n as f32) * 100.0)
            });
            println!(
                "· in {}fr x{} out {}fr | DSP {:.1} µs (max {:.0}) = {:.1}% | latency {:.2} ms (in {:.2} + ring {:.2} + out {:.2}) | ring {}..{}fr → {} | drift {:+.0} ppm | peak {:.0} dBFS | underruns {}{tuner}",
                s.in_block,
                s.in_callbacks,
                s.out_block,
                s.dsp_mean_us,
                s.dsp_max_us,
                100.0 * s.dsp_mean_us / budget_us,
                s.round_trip_ms,
                s.in_latency_ms,
                s.ring_ms,
                s.out_latency_ms,
                s.ring_min.min(s.ring_max),
                s.ring_max,
                s.setpoint,
                s.drift_ppm,
                lin_to_db(s.input_peak),
                s.underruns
            );
            last_report = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let s = engine.stats();
    println!(
        "\nstopped after {:.1} s: {} input / {} output callbacks, {} notes, underruns {} (+{} while priming), overruns {}, resyncs {}, clock drift compensated {:+.1} ppm",
        started.elapsed().as_secs_f32(),
        s.in_callbacks,
        s.out_callbacks,
        notes,
        s.underruns,
        s.priming,
        s.overruns,
        s.resyncs,
        s.drift_ppm
    );
    if s.errors > 0 {
        let kinds: Vec<&str> = (0..ERROR_KINDS.len()).filter(|b| s.error_kinds >> b & 1 == 1).map(|b| ERROR_KINDS[b]).collect();
        println!("stream notifications from CoreAudio: {} ({})", s.errors, kinds.join(", "));
    }
    Ok(())
}

/// Open strings with the spec's frequencies.
const OPEN: [(&str, f32, i32); 6] =
    [("E2", 82.41, 40), ("A2", 110.0, 45), ("D3", 146.83, 50), ("G3", 196.0, 55), ("B3", 246.94, 59), ("E4", 329.63, 64)];

fn pluck(freq: f32, secs: f32, seed: u64) -> Vec<f32> {
    let mut rng = Pcg32::new(seed);
    let mut p = Pluck::new(freq);
    p.secs = secs;
    p.amp = 0.4;
    p.pluck_pos = rng.range(0.1, 0.4);
    p.pick_noise = rng.range(0.1, 0.5);
    p.render(SR, &mut rng)
}

fn chord_notes(names: &[&str]) -> Vec<i32> {
    names.iter().map(|n| note::parse_note(n).expect("valid note")).collect()
}

fn strummed(midis: &[i32], seed: u64) -> Vec<f32> {
    let mut rng = Pcg32::new(seed);
    let plucks: Vec<Pluck> = midis
        .iter()
        .map(|&m| {
            let mut p = Pluck::new(note::midi_to_freq(m as f32, 440.0));
            p.secs = 1.5;
            p.amp = 0.22;
            p.pluck_pos = rng.range(0.1, 0.3);
            p
        })
        .collect();
    strum(&plucks, SR, 12.0, &mut rng)
}

const CHORDS: [(&str, &[&str]); 5] = [
    ("E", &["E2", "B2", "E3", "G#3", "B3", "E4"]),
    ("Am", &["A2", "E3", "A3", "C4", "E4"]),
    ("G", &["G2", "B2", "D3", "G3", "B3", "G4"]),
    ("C", &["C3", "E3", "G3", "C4", "E4"]),
    ("D", &["D3", "A3", "D4", "F#4"]),
];

fn selftest() -> Result<()> {
    let mut failures = 0;
    println!("Single notes — plucked strings after silence, fed in {BLOCK}-sample buffers, worst of 4 alignments\n");
    println!("{:<5} {:<5} {:>9} {:>10} {:>11} {:>8}", "algo", "note", "attack", "confirmed", "period-min", "result");
    for (algo, algorithm) in [("MPM", Algorithm::Mpm), ("YIN", Algorithm::Yin)] {
        let cfg = TrackerConfig { algorithm, ..TrackerConfig::default() };
        for (k, (label, f, midi)) in OPEN.into_iter().enumerate() {
            let (mut timing, mut confirm, mut ok) = (0f32, 0f32, true);
            for offset in [0usize, 21, 42, 63] {
                let lead = 4800 + offset;
                let mut x = vec![0.0; lead];
                x.extend(pluck(f, 0.3, (k * 10 + offset) as u64));
                let mut tracker = NoteTracker::new(cfg);
                let mut ons = Vec::new();
                for chunk in x.chunks(BLOCK) {
                    tracker.process(chunk, |e| {
                        if let TrackerEvent::NoteOn { onset_sample, sample, midi, .. } = e {
                            ons.push((onset_sample, sample, midi));
                        }
                    });
                }
                match ons.as_slice() {
                    [(onset, sample, m)] if *m == midi => {
                        timing = timing.max((*onset as f32 - lead as f32) * 1000.0 / SR);
                        confirm = confirm.max((*sample - lead as u64) as f32 * 1000.0 / SR);
                    }
                    _ => ok = false,
                }
            }
            let physics = 1000.0 * match algorithm {
                Algorithm::Mpm => Mpm::new(cfg.pitch).min_window_for(f),
                Algorithm::Yin => opensmith_dsp::Yin::new(cfg.pitch).min_window_for(f),
            } as f32
                / SR;
            failures += !ok as usize;
            println!(
                "{algo:<5} {label:<5} {timing:>7.2}ms {confirm:>8.2}ms {physics:>9.2}ms {:>8}",
                if ok { "✔" } else { "✘ FAIL" }
            );
        }
    }

    println!("\nChords — strummed (12 ms between strings), verified against the expected voicing\n");
    let mut verifier = ChordVerifier::new(ChordConfig::default());
    for (seed, (name, notes)) in CHORDS.iter().enumerate() {
        let m = chord_notes(notes);
        let x = strummed(&m, seed as u64);
        let last = ((m.len() - 1) as f32 * 12.0e-3 * SR) as usize;
        let accepted = (1..)
            .map(|k| last + k * BLOCK)
            .take_while(|&e| e <= x.len())
            .find(|&e| verifier.verify(&x[..e], &m, 440.0).accepted);
        let wrong = CHORDS.iter().filter(|(other, _)| other != name).all(|(_, o)| {
            !verifier.verify(&x[..last + 4096], &chord_notes(o), 440.0).accepted
        });
        let ok = accepted.is_some() && wrong;
        failures += !ok as usize;
        println!(
            "{name:<3} accepted {:>6} after last string, other chords rejected: {}  {}",
            accepted.map_or("never".into(), |e| format!("{:.1}ms", (e - last) as f32 * 1000.0 / SR)),
            if wrong { "yes" } else { "NO" },
            if ok { "✔" } else { "✘ FAIL" }
        );
    }

    println!("\nCPU — tracker + gate/overdrive/cabinet per {BLOCK}-sample buffer");
    let mut x = Vec::new();
    for (k, (_, f, _)) in OPEN.into_iter().enumerate() {
        let at = x.len();
        mix_into(&mut x, &pluck(f, 0.5, k as u64), at, 1.0);
    }
    let mut tracker = NoteTracker::new(TrackerConfig::default());
    let mut chain = GuitarChain::new(SR, ChainParams::default(), None);
    let mut times = Vec::new();
    let mut buf = [0f32; BLOCK];
    for chunk in x.chunks(BLOCK) {
        let t = Instant::now();
        tracker.process(chunk, |_| {});
        buf[..chunk.len()].copy_from_slice(chunk);
        chain.process(&mut buf[..chunk.len()]);
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let budget = BLOCK as f64 / SR as f64 * 1e6;
    println!(
        "mean {mean:.1} µs, p99.9 {:.1} µs, max {:.1} µs — buffer period {budget:.0} µs ({:.1}% load)",
        times[times.len() * 999 / 1000],
        times[times.len() - 1],
        100.0 * mean / budget
    );

    if failures > 0 {
        bail!("{failures} self-test check(s) failed");
    }
    println!("\nAll self-test checks passed.");
    Ok(())
}

fn generate(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let write = |name: &str, x: &[f32]| -> Result<()> {
        let path = dir.join(name);
        wav::write_mono_f32(&path, x, SR as u32).with_context(|| format!("writing {}", path.display()))?;
        println!("{}  ({:.2} s)", path.display(), x.len() as f32 / SR);
        Ok(())
    };
    for (k, (label, f, _)) in OPEN.into_iter().enumerate() {
        write(&format!("open_{}_sine.wav", label.to_lowercase()), &sine(f, SR, 2.0, 0.5))?;
        write(&format!("open_{}_pluck.wav", label.to_lowercase()), &pluck(f, 2.0, k as u64))?;
    }
    for (seed, (name, notes)) in CHORDS.iter().enumerate() {
        write(&format!("chord_{name}.wav"), &strummed(&chord_notes(notes), seed as u64))?;
    }
    let mut riff = Vec::new();
    for (k, midi) in (40..=64).enumerate() {
        let n = pluck(note::midi_to_freq(midi as f32, 440.0), 0.125, 100 + k as u64);
        mix_into(&mut riff, &n, (k as f32 * 0.125 * SR) as usize, 1.0);
    }
    write("riff_chromatic_120bpm.wav", &riff)
}

fn render(input: &Path, output: &Path, tone: &ToneArgs) -> Result<()> {
    let (x, rate) = wav::read_mono(input).with_context(|| format!("reading {}", input.display()))?;
    let x = if rate as f32 == SR { x } else { resample(&x, rate as f32, SR, 32) };
    let mut tracker = NoteTracker::new(TrackerConfig::default());
    let mut chain = GuitarChain::new(SR, tone.chain(), tone.ir()?.as_ref());
    let mut y = Vec::with_capacity(x.len());
    let mut events = Vec::new();
    for chunk in x.chunks(BLOCK) {
        tracker.process(chunk, |e| events.push(e));
        let mut b = chunk.to_vec();
        chain.process(&mut b);
        y.extend(b);
    }
    wav::write_mono_f32(output, &y, SR as u32).with_context(|| format!("writing {}", output.display()))?;
    let tuning = Tuning::STANDARD;
    let mut hand = 0;
    for e in events {
        if let TrackerEvent::NoteOn { onset_sample, midi, cents, legato, .. } = e {
            let place = tuning.resolve(midi, hand).map_or("-".to_string(), |p| {
                hand = p.fret;
                format!("{} fret {}", STRING_NAMES[p.string as usize], p.fret)
            });
            println!(
                "{:8.3} s  {:<4} {place:<14} {cents:+5.1}¢{}",
                onset_sample as f32 / SR,
                note::note_name(midi),
                if legato { " legato" } else { "" }
            );
        }
    }
    println!("→ {} ({:.1} s)", output.display(), y.len() as f32 / SR);
    Ok(())
}
