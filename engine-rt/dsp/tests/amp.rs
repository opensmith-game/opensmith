//! Amp models: bounded and evenly loud, gain distorts, tone knobs shape the spectrum, echoes and
//! room tail where expected.

use opensmith_dsp::fft::FftBank;
use opensmith_dsp::synth::{sine, Pcg32, Pluck};
use opensmith_dsp::{AmpKnobs, AmpModel, ChainParams, GuitarChain};

const SR: f32 = 48_000.0;
const MODELS: [AmpModel; 4] = [AmpModel::Clean, AmpModel::Crunch, AmpModel::Lead, AmpModel::Metal];

fn run(model: AmpModel, knobs: AmpKnobs, x: &[f32]) -> Vec<f32> {
    let mut chain = GuitarChain::new(SR, ChainParams::default().with_amp(model, knobs), None);
    let mut y = x.to_vec();
    for block in y.chunks_mut(64) {
        chain.process(block);
    }
    y
}

/// A strummed E major and a single-note line, as a cable delivers them (peaks ~0.5).
fn playing() -> Vec<f32> {
    let mut rng = Pcg32::new(3);
    let mut x = vec![0f32; (4.0 * SR) as usize];
    for (k, midi) in [40, 47, 52, 56, 59, 64].into_iter().enumerate() {
        let note = Pluck { secs: 2.0, ..Pluck::new(440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)) }.render(SR, &mut rng);
        let at = k * 400;
        for (i, v) in note.iter().enumerate() {
            x[at + i] += v * 0.35;
        }
    }
    for (k, midi) in [52, 55, 57, 59, 62, 64].into_iter().enumerate() {
        let note = Pluck { secs: 0.3, ..Pluck::new(440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)) }.render(SR, &mut rng);
        let at = (2.2 * SR) as usize + k * (0.3 * SR) as usize;
        for (i, v) in note.iter().enumerate() {
            x[at + i] += v;
        }
    }
    x
}

fn rms_db(x: &[f32]) -> f32 {
    10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).log10()
}

/// Energy (dB) of `x` between `lo` and `hi` Hz.
fn band_db(x: &[f32], lo: f32, hi: f32) -> f32 {
    let n = 1 << 15;
    let mut fft = FftBank::new(n);
    let spec = fft.forward(&x[..n], n);
    let bin = SR / n as f32;
    let e: f32 = spec.iter().enumerate().filter(|(k, _)| (*k as f32 * bin) >= lo && (*k as f32 * bin) < hi).map(|(_, c)| c.norm_sqr()).sum();
    10.0 * (e + 1e-12).log10()
}

#[test]
fn every_amp_stays_bounded_and_about_as_loud_as_the_others() {
    let x = playing();
    let levels: Vec<f32> = MODELS
        .iter()
        .map(|&m| {
            let y = run(m, AmpKnobs { reverb: 0.4, delay: 0.3, ..AmpKnobs::default() }, &x);
            assert!(y.iter().all(|v| v.is_finite() && v.abs() < 1.0), "{m:?}");
            rms_db(&y)
        })
        .collect();
    println!("loudness clean/crunch/lead/metal: {levels:.1?} dB RMS");
    let (lo, hi) = levels.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    assert!(hi - lo < 6.0, "{levels:?}");
}

#[test]
fn more_gain_more_distortion() {
    let x = sine(110.0, SR, 1.0, 0.3);
    let harmonics = |gain: f32| {
        let y = run(AmpModel::Crunch, AmpKnobs { gain, gate: false, ..AmpKnobs::default() }, &x);
        band_db(&y[4800..], 300.0, 3000.0) - band_db(&y[4800..], 100.0, 120.0)
    };
    let (low, high) = (harmonics(0.0), harmonics(1.0));
    println!("harmonics over fundamental: gain 0 {low:.1} dB, gain 1 {high:.1} dB");
    assert!(high > low + 3.0);
    let clean = run(AmpModel::Clean, AmpKnobs { gain: 0.0, gate: false, ..AmpKnobs::default() }, &x);
    let metal = run(AmpModel::Metal, AmpKnobs { gain: 1.0, gate: false, ..AmpKnobs::default() }, &x);
    let ratio = |y: &[f32]| band_db(&y[4800..], 300.0, 3000.0) - band_db(&y[4800..], 100.0, 120.0);
    assert!(ratio(&metal) > ratio(&clean) + 10.0, "metal {} clean {}", ratio(&metal), ratio(&clean));
}

#[test]
fn tone_knobs_shape_the_sound() {
    let mut rng = Pcg32::new(9);
    let noise: Vec<f32> = (0..(1.5 * SR) as usize).map(|_| 0.05 * rng.gauss()).collect();
    let tone = |bass: f32, treble: f32| {
        let y = run(AmpModel::Clean, AmpKnobs { gain: 0.0, bass, treble, gate: false, ..AmpKnobs::default() }, &noise);
        (band_db(&y[4800..], 60.0, 150.0), band_db(&y[4800..], 3500.0, 6000.0))
    };
    let (bass_up, _) = tone(1.0, 0.5);
    let (bass_down, _) = tone(0.0, 0.5);
    let (_, treble_up) = tone(0.5, 1.0);
    let (_, treble_down) = tone(0.5, 0.0);
    assert!(bass_up - bass_down > 12.0, "bass {bass_up} vs {bass_down}");
    assert!(treble_up - treble_down > 12.0, "treble {treble_up} vs {treble_down}");
}

#[test]
fn the_delay_repeats_and_the_room_rings_on() {
    let mut x = vec![0f32; (3.0 * SR) as usize];
    x[100] = 0.5;
    let dry = run(AmpModel::Clean, AmpKnobs { gain: 0.0, gate: false, ..AmpKnobs::default() }, &x);
    let echo = run(AmpModel::Clean, AmpKnobs { gain: 0.0, gate: false, delay: 1.0, ..AmpKnobs::default() }, &x);
    let at = |t: f32, y: &[f32]| {
        let k = (t * SR) as usize;
        y[k - 2400..k + 2400].iter().fold(0f32, |m, v| m.max(v.abs()))
    };
    let first = at(0.38 + 0.03, &echo);
    assert!(first > 10.0 * at(0.38 + 0.03, &dry), "an echo 380 ms later");
    assert!(at(0.76 + 0.03, &echo) < first, "repeats fade");
    let room = run(AmpModel::Clean, AmpKnobs { gain: 0.0, gate: false, reverb: 1.0, ..AmpKnobs::default() }, &x);
    let tail = |t0: f32, t1: f32, y: &[f32]| rms_db(&y[(t0 * SR) as usize..(t1 * SR) as usize]);
    assert!(tail(0.5, 1.0, &room) > tail(0.5, 1.0, &dry) + 20.0, "the room rings after the note");
    assert!(tail(2.0, 2.5, &room) < tail(0.5, 1.0, &room) - 10.0, "and dies away");
}
