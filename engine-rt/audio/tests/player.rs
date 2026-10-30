//! Backing-track playback, the song clock and input timestamps, with simulated callbacks.

use std::sync::Arc;

use opensmith_audio::{Bridge, EngineConfig, PlayerCommand, Track, TAP_BLOCK};

const SR: f64 = 48_000.0;
const BLOCK: usize = 64;
const OUT_LATENCY_NS: u64 = 5_000_000;

fn bridge() -> Bridge {
    Bridge::new(&EngineConfig::default(), 1, 2, 0.0)
}

/// Left channel: frame index × 1e-6, right channel: its opposite.
fn ramp(frames: usize) -> Arc<Track> {
    let data: Vec<f32> = (0..frames).flat_map(|i| [i as f32 * 1e-6, -(i as f32) * 1e-6]).collect();
    Arc::new(Track::from_interleaved(&data, 2))
}

fn callback_ns(k: usize) -> u64 {
    (k as f64 * BLOCK as f64 / SR * 1e9) as u64
}

/// Output callbacks `k0..k0 + n`; returns the left channel.
fn run(b: &mut Bridge, k0: usize, n: usize) -> Vec<f32> {
    let mut out = vec![0f32; BLOCK * 2];
    let mut left = Vec::new();
    for k in k0..k0 + n {
        b.output.process(&mut out, callback_ns(k), OUT_LATENCY_NS);
        left.extend(out.chunks(2).map(|f| f[0]));
    }
    left
}

#[test]
fn the_backing_track_plays_sample_exact_and_the_clock_follows() {
    let mut b = bridge();
    assert!(b.monitor.send(PlayerCommand::Load { track: Some(ramp(10_000)), frame: 0, generation: 1 }));
    assert!(b.monitor.send(PlayerCommand::Play));
    let left = run(&mut b, 0, 100);
    for (i, v) in left.iter().enumerate() {
        assert!((v - i as f32 * 1e-6).abs() < 1e-9, "frame {i}: {v}");
    }
    let clock = b.monitor.clock();
    assert!(clock.valid && clock.playing && !clock.ended);
    assert_eq!(clock.generation, 1);
    assert_eq!(clock.frame, 99 * BLOCK as i64, "position of the last callback's first frame");
    assert_eq!(clock.heard_ns, callback_ns(99) + OUT_LATENCY_NS);
    let heard = clock.heard_ns as f64 + 10e6;
    assert!((clock.time_at(heard, SR) - (99.0 * BLOCK as f64 / SR + 0.010)).abs() < 1e-9);
    assert!((clock.host_at(clock.time_at(heard, SR), SR) - heard).abs() < 1e-3);
}

#[test]
fn a_count_in_is_silent_and_the_song_starts_on_time() {
    let mut b = bridge();
    let lead = 10 * BLOCK + 7;
    b.monitor.send(PlayerCommand::Load { track: Some(ramp(5_000)), frame: -(lead as i64), generation: 1 });
    b.monitor.send(PlayerCommand::Play);
    let left = run(&mut b, 0, 40);
    assert!(left[..lead].iter().all(|&v| v == 0.0));
    for (i, v) in left.iter().enumerate().skip(lead) {
        assert!((v - (i - lead) as f32 * 1e-6).abs() < 1e-9, "frame {i}");
    }
}

#[test]
fn seeks_are_tagged_so_the_caller_knows_when_they_apply() {
    let mut b = bridge();
    b.monitor.send(PlayerCommand::Load { track: Some(ramp(48_000)), frame: 0, generation: 1 });
    b.monitor.send(PlayerCommand::Play);
    run(&mut b, 0, 10);
    b.monitor.send(PlayerCommand::Seek { frame: 24_000, generation: 2 });
    assert_eq!(b.monitor.clock().generation, 1, "applied by the next callback only");
    let left = run(&mut b, 10, 1);
    let clock = b.monitor.clock();
    assert_eq!((clock.generation, clock.frame), (2, 24_000));
    assert!((left[0] - 0.024).abs() < 1e-7);
}

#[test]
fn pausing_freezes_the_clock_and_the_end_stops_the_player() {
    let mut b = bridge();
    b.monitor.send(PlayerCommand::Load { track: Some(ramp(1_000)), frame: 0, generation: 1 });
    b.monitor.send(PlayerCommand::Play);
    run(&mut b, 0, 5);
    b.monitor.send(PlayerCommand::Pause);
    let left = run(&mut b, 5, 3);
    let clock = b.monitor.clock();
    assert!(!clock.playing && left.iter().all(|&v| v == 0.0));
    assert_eq!(clock.time_at(1e12, SR), 5.0 * BLOCK as f64 / SR);

    b.monitor.send(PlayerCommand::Play);
    let left = run(&mut b, 8, 20);
    let clock = b.monitor.clock();
    assert!(clock.ended && !clock.playing);
    assert!(left[1_000 - 5 * BLOCK..].iter().all(|&v| v == 0.0), "silence after the end");

    b.monitor.send(PlayerCommand::Play);
    run(&mut b, 28, 1);
    assert!(!b.monitor.clock().playing, "an ended track needs a seek before playing again");
    b.monitor.send(PlayerCommand::Seek { frame: 0, generation: 2 });
    b.monitor.send(PlayerCommand::Play);
    run(&mut b, 29, 1);
    assert!(b.monitor.clock().playing);
}

#[test]
fn input_samples_get_their_capture_time_and_reach_the_tap() {
    let mut b = bridge();
    let latency = 3_000_000;
    let signal: Vec<f32> = (0..BLOCK * 50).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
    for (k, block) in signal.chunks(BLOCK).enumerate() {
        // Sample k·BLOCK is captured at 1 s + k·BLOCK/SR, the callback runs `latency` later.
        b.input.process(block, 1_000_000_000 + callback_ns(k) + latency, latency);
    }
    for i in [0.0, 100.0, 3199.0] {
        let t = b.monitor.input_time_ns(i).unwrap();
        assert!((t - (1e9 + i / SR * 1e9)).abs() < 2.0, "sample {i}: {t}");
    }
    assert!((b.monitor.input_index_at(1.05e9).unwrap() - 2400.0).abs() < 1e-3);

    let mut got = Vec::new();
    let mut next = 0u64;
    b.monitor.drain_tap(|block| {
        assert_eq!(block.first, next, "contiguous blocks");
        next += block.len as u64;
        got.extend_from_slice(block.samples());
    });
    assert_eq!(got.len(), signal.len() / TAP_BLOCK * TAP_BLOCK);
    assert_eq!(&got[..], &signal[..got.len()]);
}

#[test]
fn mono_outputs_get_both_channels() {
    let mut b = Bridge::new(&EngineConfig::default(), 1, 1, 0.0);
    let data: Vec<f32> = (0..256).flat_map(|_| [0.4, 0.2]).collect();
    b.monitor.send(PlayerCommand::Load { track: Some(Arc::new(Track::from_interleaved(&data, 2))), frame: 0, generation: 1 });
    b.monitor.send(PlayerCommand::Play);
    let mut out = vec![0f32; BLOCK];
    b.output.process(&mut out, 0, 0);
    assert!(out.iter().all(|&v| (v - 0.3).abs() < 1e-6));
}

#[test]
fn the_input_gain_feeds_detection_tap_and_meter() {
    let mut b = bridge();
    b.monitor.set_input_gain(2.0);
    let block = vec![0.1f32; BLOCK];
    for k in 0..20 {
        b.input.process(&block, callback_ns(k) + 1_000_000, 1_000_000);
    }
    assert!((b.monitor.take_peak() - 0.2).abs() < 1e-6);
    assert_eq!(b.monitor.take_peak(), 0.0, "the meter restarts at each reading");
    let mut tapped = Vec::new();
    b.monitor.drain_tap(|t| tapped.extend_from_slice(t.samples()));
    assert!(!tapped.is_empty() && tapped.iter().all(|&v| (v - 0.2).abs() < 1e-6));
}

/// Guitar in, monitored guitar out, for `blocks` callbacks from callback index `k0`; loudest
/// output sample over the second half.
fn duplex(b: &mut Bridge, k0: usize, blocks: usize) -> f32 {
    let signal: Vec<f32> = (0..BLOCK * blocks).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
    let mut out = vec![0f32; BLOCK * 2];
    let mut loudest = 0f32;
    for (j, block) in signal.chunks(BLOCK).enumerate() {
        let t = callback_ns(k0 + j);
        b.input.process(block, t + 500_000, 500_000);
        b.output.process(&mut out, t + 700_000, 4_000_000);
        if j > blocks / 2 {
            loudest = out.iter().fold(loudest, |m, v| m.max(v.abs()));
        }
    }
    loudest
}

#[test]
fn the_guitar_volume_in_the_headphones_changes_on_the_fly() {
    let mut b = Bridge::new(&EngineConfig::default(), 1, 2, 1.0);
    assert!(b.monitor.monitor_enabled());
    assert!(duplex(&mut b, 0, 200) > 0.01);
    b.monitor.set_monitor_gain(0.0);
    assert_eq!(duplex(&mut b, 200, 200), 0.0);

    // Without the USB guitar cable the input is the Mac's microphone: monitoring stays off.
    let mut mic = bridge();
    mic.monitor.set_monitor_gain(1.0);
    assert!(!mic.monitor.monitor_enabled());
    assert_eq!(duplex(&mut mic, 0, 200), 0.0);
}

/// Stereo track: a sine of `freq` Hz, amplitude `amp`, `secs` long.
fn sine(freq: f64, secs: f64, amp: f32) -> Arc<Track> {
    let n = (secs * SR) as usize;
    let data: Vec<f32> = (0..n)
        .flat_map(|i| {
            let v = amp * (std::f64::consts::TAU * freq * i as f64 / SR).sin() as f32;
            [v, v]
        })
        .collect();
    Arc::new(Track::from_interleaved(&data, 2))
}

/// Power of `x` at `freq` (Goertzel).
fn tone(x: &[f32], freq: f64) -> f64 {
    let c = 2.0 * (std::f64::consts::TAU * freq / SR).cos();
    let (mut s1, mut s2) = (0.0, 0.0);
    for &v in x {
        let s = v as f64 + c * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    s1 * s1 + s2 * s2 - c * s1 * s2
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| v as f64 * v as f64).sum::<f64>() / x.len() as f64).sqrt()
}

#[test]
fn slowing_down_keeps_the_pitch_and_the_level() {
    for rate in [0.5, 0.75] {
        let mut b = bridge();
        b.monitor.send(PlayerCommand::Load { track: Some(sine(440.0, 6.0, 0.5)), frame: 0, generation: 1 });
        b.monitor.send(PlayerCommand::Rate(rate));
        b.monitor.send(PlayerCommand::Play);
        let n = 1500;
        let left = run(&mut b, 0, n);
        let clock = b.monitor.clock();
        assert_eq!(clock.rate, rate);
        let expected = ((n - 1) * BLOCK) as f64 * rate;
        assert!((clock.frame as f64 - expected).abs() <= 1.0, "rate {rate}: clock at {} for {expected}", clock.frame);
        let heard = clock.time_at(clock.heard_ns as f64 + 0.1e9, SR) - clock.frame as f64 / SR;
        assert!((heard - 0.1 * rate).abs() < 1e-6, "the clock runs at the playback speed");

        let body = &left[4_800..];
        let (kept, shifted) = (tone(body, 440.0), tone(body, 440.0 * rate));
        assert!(kept > 1_000.0 * shifted, "rate {rate}: 440 Hz {kept:.3e}, {} Hz {shifted:.3e}", 440.0 * rate);
        for (k, w) in body.chunks_exact(960).enumerate() {
            let db = 20.0 * (rms(w) / (0.5 / 2f64.sqrt())).log10();
            assert!(db.abs() < 1.0, "rate {rate}: window {k} at {db:.2} dB");
        }
    }
}

#[test]
fn slowed_music_stays_with_the_clock() {
    // A 2 ms click every quarter second, silence between.
    let period = 12_000usize;
    let first = 6_000usize;
    let frames = 6 * SR as usize;
    let mut data = vec![0f32; 2 * frames];
    let clicks: Vec<usize> = (first..frames).step_by(period).collect();
    for &c in &clicks {
        for v in &mut data[2 * c..2 * (c + 96)] {
            *v = 0.8;
        }
    }
    let rate = 0.75;
    let mut b = bridge();
    b.monitor.send(PlayerCommand::Load { track: Some(Arc::new(Track::from_interleaved(&data, 2))), frame: 0, generation: 1 });
    b.monitor.send(PlayerCommand::Rate(rate));
    b.monitor.send(PlayerCommand::Play);
    // Song position of every output frame, from the clock of its callback.
    let (mut left, mut position) = (Vec::new(), Vec::new());
    let mut out = vec![0f32; BLOCK * 2];
    for k in 0..2_000 {
        b.output.process(&mut out, callback_ns(k), OUT_LATENCY_NS);
        let clock = b.monitor.clock();
        for (j, f) in out.chunks(2).enumerate() {
            left.push(f[0]);
            position.push(clock.frame as f64 + j as f64 * rate);
        }
    }
    let end = *position.last().unwrap() as usize;
    let mut heard = vec![false; clicks.len()];
    let mut quiet = 0;
    for (i, &v) in left.iter().enumerate() {
        if v.abs() < 0.2 {
            quiet += 1;
            continue;
        }
        if quiet >= 480 {
            let p = position[i];
            let (k, error) = clicks
                .iter()
                .enumerate()
                .map(|(k, &c)| (k, if p < c as f64 { p - c as f64 } else { (p - (c + 96) as f64).max(0.0) }))
                .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap();
            assert!(error.abs() < 0.030 * SR, "click {k} heard {:.1} ms off the clock", error / SR * 1e3);
            heard[k] = true;
        }
        quiet = 0;
    }
    let due = clicks.iter().filter(|&&c| c + 2_000 < end).count();
    assert!(heard[..due].iter().all(|&h| h), "every click is heard: {heard:?}");
}

#[test]
fn a_loop_plays_its_section_again_with_a_smooth_join() {
    let mut b = bridge();
    b.monitor.send(PlayerCommand::Load { track: Some(ramp(48_000)), frame: 0, generation: 1 });
    b.monitor.send(PlayerCommand::Loop(Some((2_000, 10_000))));
    b.monitor.send(PlayerCommand::Seek { frame: 2_000, generation: 2 });
    b.monitor.send(PlayerCommand::Play);
    let left = run(&mut b, 0, 300);
    let clock = b.monitor.clock();
    assert_eq!(clock.loops, 2, "two times back to the start in 19 200 frames");
    assert!((2_000..10_000).contains(&clock.frame) && clock.playing && !clock.ended);
    for (i, v) in left[..8_000].iter().enumerate() {
        assert!((v - (2_000 + i) as f32 * 1e-6).abs() < 1e-9, "first pass, frame {i}");
    }
    let fade = (0.012 * SR) as usize;
    for (i, v) in left.iter().enumerate().take(16_000).skip(8_000 + fade) {
        assert!((v - (i - 6_000) as f32 * 1e-6).abs() < 1e-9, "second pass, frame {i}");
    }
    let jump = left[7_990..8_000 + fade + 10].windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max);
    assert!(jump < 5e-5, "the end fades into the start: largest step {jump:e} (a cut: 8e-3)");

    // Without the loop the track plays on from where it is.
    b.monitor.send(PlayerCommand::Loop(None));
    run(&mut b, 300, 200);
    assert!(b.monitor.clock().frame > 10_000);
}

#[test]
fn changing_speed_does_not_click() {
    let mut b = bridge();
    b.monitor.send(PlayerCommand::Load { track: Some(sine(440.0, 8.0, 0.5)), frame: 0, generation: 1 });
    b.monitor.send(PlayerCommand::Play);
    let mut left = run(&mut b, 0, 100);
    let mut k = 100;
    for rate in [0.75, 1.0, 0.5, 0.9, 1.0] {
        b.monitor.send(PlayerCommand::Rate(rate));
        left.extend(run(&mut b, k, 150));
        k += 150;
        assert_eq!(b.monitor.clock().rate, rate);
    }
    let slope = 0.5 * std::f32::consts::TAU * 440.0 / SR as f32;
    let step = left.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max);
    assert!(step < 1.3 * slope, "largest step {step} for a sine slope of {slope}");
}

#[test]
fn the_amp_changes_while_playing() {
    use opensmith_dsp::{AmpKnobs, AmpModel, ChainParams};

    let mut b = Bridge::new(&EngineConfig::default(), 1, 2, 1.0);
    let before = duplex(&mut b, 0, 200);
    let quiet = ChainParams::default().with_amp(AmpModel::Clean, AmpKnobs { volume: 0.1, ..AmpKnobs::default() });
    assert!(b.monitor.set_chain(quiet));
    let after = duplex(&mut b, 200, 200);
    assert!(after > 0.0 && after < before / 4.0, "{after} vs {before}");
}
