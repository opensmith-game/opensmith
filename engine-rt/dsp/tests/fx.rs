//! Guitar FX chain: noise gate, ADAA overdrive, partitioned cabinet convolution, IR loading,
//! safety limiter.

mod common;

use common::*;
use opensmith_dsp::fx::cabinet::{CabinetIr, Convolver};
use opensmith_dsp::fx::drive::{AdaaTanh, DriveParams, Overdrive};
use opensmith_dsp::fx::gate::{GateParams, NoiseGate};
use opensmith_dsp::synth::{sine, white_noise, Pcg32};
use opensmith_dsp::{db_to_lin, lin_to_db, ChainParams, GuitarChain};
use realfft::RealFftPlanner;

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// Magnitude spectrum (Hann window), `n` = x.len() must be a power of two.
fn spectrum(x: &[f32]) -> Vec<f32> {
    let n = x.len();
    let fft = RealFftPlanner::<f32>::new().plan_fft_forward(n);
    let mut buf: Vec<f32> =
        x.iter().enumerate().map(|(i, v)| v * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos())).collect();
    let mut out = fft.make_output_vec();
    fft.process(&mut buf, &mut out).unwrap();
    out.iter().map(|c| c.norm()).collect()
}

#[test]
fn gate_mutes_noise_and_passes_playing() {
    let mut rng = Pcg32::new(1);
    let mut gate = NoiseGate::new(SR, GateParams::default());
    let mut noise = white_noise(&mut rng, 96_000, db_to_lin(-70.0));
    let input_rms = rms(&noise);
    gate.process(&mut noise);
    let out_db = lin_to_db(rms(&noise[72_000..]));
    assert!(out_db < lin_to_db(input_rms) - 60.0, "noise only attenuated to {out_db:.1} dBFS");

    let mut tone = sine(220.0, SR, 0.5, db_to_lin(-20.0));
    let reference = rms(&tone[4800..]);
    gate.process(&mut tone);
    let diff = lin_to_db(rms(&tone[4800..])) - lin_to_db(reference);
    assert!(diff.abs() < 0.1, "open gate changes level by {diff:.2} dB");
}

#[test]
fn gate_has_hysteresis_hold_and_no_clicks() {
    let p = GateParams::default();
    let mut gate = NoiseGate::new(SR, p);
    // A note decaying slowly through both thresholds: exactly one opening and one closing.
    let n = (3.0 * SR) as usize;
    let mut transitions = 0;
    let mut was_open = false;
    let mut last_gain = gate.gain();
    let mut max_step = 0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        let level = db_to_lin(-30.0 - 15.0 * t);
        let mut x = [level * (std::f32::consts::TAU * 196.0 * t).sin()];
        gate.process(&mut x);
        if gate.is_open() != was_open {
            transitions += 1;
            was_open = gate.is_open();
        }
        max_step = max_step.max((gate.gain() - last_gain).abs());
        last_gain = gate.gain();
    }
    assert_eq!(transitions, 2, "gate chattered");
    assert!(!gate.is_open());
    // Gain moves by at most one attack step per sample (0.5 ms ramp ≈ 4 % per sample).
    assert!(max_step < 0.05, "gain jumped by {max_step}");
}

#[test]
fn overdrive_is_bounded_dc_free_and_distorts_with_drive() {
    let mut thd = Vec::new();
    for drive in [0.1, 0.5, 0.9] {
        let p = DriveParams { drive, level_db: 0.0, tone: 1.0, ..DriveParams::default() };
        let mut od = Overdrive::new(SR, p);
        let mut x = sine(220.0, SR, 1.0, 0.5);
        od.process(&mut x);
        let tail = &x[24_000..24_000 + 16_384];
        assert!(tail.iter().all(|v| v.is_finite() && v.abs() <= 1.5));
        // Exactly 110 periods of 220 Hz, so no partial period biases the mean.
        let whole = &x[24_000..48_000];
        let mean = whole.iter().sum::<f32>() / whole.len() as f32;
        assert!(mean.abs() < 1e-4, "DC offset {mean}");
        let s = spectrum(tail);
        let bin = |f: f32| (f / SR * tail.len() as f32).round() as usize;
        let fund: f32 = s[bin(220.0) - 2..=bin(220.0) + 2].iter().map(|v| v * v).sum();
        let harm: f32 = (2..20).map(|h| s[bin(220.0 * h as f32) - 2..=bin(220.0 * h as f32) + 2].iter().map(|v| v * v).sum::<f32>()).sum();
        thd.push((harm / fund).sqrt());
    }
    println!("THD at drive 0.1/0.5/0.9: {thd:?}");
    assert!(thd[0] < thd[1] && thd[1] < thd[2], "THD must grow with drive");
    // Extreme input stays bounded.
    let mut od = Overdrive::new(SR, DriveParams { drive: 1.0, level_db: 0.0, ..DriveParams::default() });
    let mut x: Vec<f32> = (0..48_000).map(|i| if (i / 50) % 2 == 0 { 10.0 } else { -10.0 }).collect();
    od.process(&mut x);
    assert!(x.iter().all(|v| v.is_finite() && v.abs() < 2.0));
}

#[test]
fn adaa_cuts_aliasing_of_the_clipper() {
    // A 4.7 kHz sine driven hard: harmonics above Nyquist fold back between the harmonics.
    let n = 16_384;
    let f = 4700.0f32;
    let mut adaa = AdaaTanh::new(0.2);
    let naive_ref = AdaaTanh::new(0.2);
    let drive = 8.0;
    let input: Vec<f64> = (0..n + 2048).map(|i| drive * (std::f64::consts::TAU * f as f64 * i as f64 / SR as f64).sin()).collect();
    let a: Vec<f32> = input.iter().map(|&u| adaa.process(u) as f32).collect();
    let b: Vec<f32> = input.iter().map(|&u| naive_ref.naive(u) as f32).collect();
    let alias_ratio = |y: &[f32]| {
        let s = spectrum(&y[2048..2048 + n]);
        let bin_hz = SR / n as f32;
        let (mut harmonic, mut alias) = (0f32, 0f32);
        for (k, m) in s.iter().enumerate().skip(3) {
            let freq = k as f32 * bin_hz;
            let h = freq / f;
            if (h - h.round()).abs() * f < 4.0 * bin_hz {
                harmonic += m * m;
            } else {
                alias += m * m;
            }
        }
        alias / harmonic
    };
    let (ra, rb) = (alias_ratio(&a), alias_ratio(&b));
    println!("aliasing: ADAA {:.1} dB, naive {:.1} dB", 10.0 * ra.log10(), 10.0 * rb.log10());
    assert!(ra < rb * 0.25, "ADAA must cut aliasing by > 6 dB");
}

fn naive_convolution(x: &[f32], h: &[f32]) -> Vec<f32> {
    (0..x.len()).map(|n| (0..h.len().min(n + 1)).map(|k| h[k] as f64 * x[n - k] as f64).sum::<f64>() as f32).collect()
}

#[test]
fn partitioned_convolution_equals_direct_convolution() {
    let mut rng = Pcg32::new(9);
    for ir_len in [1usize, 37, 64, 65, 200, 1000, 4103] {
        let h: Vec<f32> = (0..ir_len).map(|i| rng.range(-1.0, 1.0) * (-(i as f32) / 800.0).exp()).collect();
        let x: Vec<f32> = (0..3000).map(|_| rng.range(-1.0, 1.0)).collect();
        let want = naive_convolution(&x, &h);
        let mut conv = Convolver::new(&h, 64);
        let mut got = Vec::with_capacity(x.len());
        let mut i = 0;
        while i < x.len() {
            let len = (1 + rng.next_u32() as usize % 300).min(x.len() - i);
            let mut block = x[i..i + len].to_vec();
            conv.process(&mut block);
            got.extend(block);
            i += len;
        }
        let scale = want.iter().fold(0f32, |a, v| a.max(v.abs()));
        let err = got.iter().zip(&want).fold(0f32, |a, (g, w)| a.max((g - w).abs()));
        assert!(err <= 1e-5 * scale + 1e-6, "IR {ir_len}: max error {err} (scale {scale})");
    }
}

#[test]
fn convolution_adds_no_latency() {
    let ir = CabinetIr::builtin_4x12(SR);
    let mut conv = Convolver::new(&ir.samples, 64);
    let mut x = vec![0.0; ir.samples.len() + 64];
    x[0] = 1.0;
    conv.process(&mut x);
    for (k, (a, b)) in x.iter().zip(&ir.samples).enumerate() {
        assert!((a - b).abs() < 1e-5, "tap {k}: {a} vs {b}");
    }
}

fn response_db(ir: &[f32], f: f32) -> f32 {
    let (mut re, mut im) = (0f64, 0f64);
    for (n, &h) in ir.iter().enumerate() {
        let w = std::f64::consts::TAU * f as f64 * n as f64 / SR as f64;
        re += h as f64 * w.cos();
        im -= h as f64 * w.sin();
    }
    (10.0 * (re * re + im * im).log10()) as f32
}

#[test]
fn builtin_cabinet_has_a_speaker_response() {
    let ir = CabinetIr::builtin_4x12(SR).samples;
    let at = |f| response_db(&ir, f);
    println!("cab: 40 Hz {:.1}, 110 Hz {:.1}, 1 kHz {:.1}, 2.2 kHz {:.1}, 10 kHz {:.1} dB", at(40.0), at(110.0), at(1000.0), at(2200.0), at(10_000.0));
    assert!((0..200).map(|k| at(20.0 + k as f32 * 100.0)).all(|db| db <= 0.05), "normalised to 0 dB peak");
    assert!(at(110.0) > at(40.0) + 6.0, "low-cut below the resonance");
    assert!(at(10_000.0) < at(1000.0) - 20.0, "speaker roll-off");
    assert!(at(2200.0) > at(1000.0), "presence bump");
}

#[test]
fn wav_impulse_response_is_loaded_resampled_and_normalised() {
    // 24-bit stereo IR at 44.1 kHz: a decaying noise burst.
    let mut rng = Pcg32::new(2);
    let src_len = 4410;
    let ir: Vec<f32> = (0..src_len).map(|i| rng.range(-1.0, 1.0) * (-(i as f32) / 300.0).exp() * 0.5).collect();
    let path = std::env::temp_dir().join(format!("opensmith-ir-{}.wav", std::process::id()));
    let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 24, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&path, spec).unwrap();
    for &v in &ir {
        let s = (v * 8_388_607.0) as i32;
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();

    let loaded = CabinetIr::from_wav(&path, SR, 500.0).unwrap();
    std::fs::remove_file(&path).ok();
    let expected_len = (src_len as f32 * SR / 44_100.0) as usize;
    assert!(loaded.samples.len() <= expected_len + 1 && loaded.samples.len() > expected_len / 2, "len {}", loaded.samples.len());
    let peak = (0..400).map(|k| response_db(&loaded.samples, 20.0 + k as f32 * 50.0)).fold(f32::MIN, f32::max);
    assert!(peak <= 0.05 && peak > -1.0, "peak response {peak} dB");
    let mut conv = Convolver::new(&loaded.samples, 64);
    let mut x = sine(440.0, SR, 0.2, 0.5);
    conv.process(&mut x);
    assert!(x.iter().all(|v| v.is_finite()));
}

#[test]
fn chain_never_reaches_full_scale() {
    let p = ChainParams {
        input_gain_db: 12.0,
        drive: DriveParams { drive: 1.0, level_db: 12.0, ..DriveParams::default() },
        output_db: 12.0,
        ..ChainParams::default()
    };
    let mut chain = GuitarChain::new(SR, p, None);
    let mut x: Vec<f32> = (0..96_000).map(|i| if (i / 80) % 2 == 0 { 4.0 } else { -4.0 }).collect();
    for block in x.chunks_mut(64) {
        chain.process(block);
    }
    let peak = x.iter().fold(0f32, |a, v| a.max(v.abs()));
    println!("chain peak with everything at max: {peak:.4}");
    assert!(peak < 1.0 && x.iter().all(|v| v.is_finite()));
}

#[test]
fn chain_with_gate_and_drive_bypassed_is_the_cabinet_with_zero_latency() {
    let p = ChainParams {
        gate: GateParams { enabled: false, ..GateParams::default() },
        drive: DriveParams { enabled: false, ..DriveParams::default() },
        ..ChainParams::default()
    };
    let ir = CabinetIr::builtin_4x12(SR);
    let mut chain = GuitarChain::new(SR, p, Some(&ir));
    let mut x = vec![0.0; 1024];
    x[0] = 0.5;
    for block in x.chunks_mut(64) {
        chain.process(block);
    }
    for (k, (y, h)) in x.iter().zip(&ir.samples).enumerate() {
        assert!((y - 0.5 * h).abs() < 1e-5, "sample {k}");
    }
}
