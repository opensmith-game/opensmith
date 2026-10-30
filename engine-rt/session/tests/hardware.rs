//! On the real CoreAudio devices (run with `cargo test -p opensmith-session -- --ignored`):
//! the song clock advances in step with the host clock the session reads.

use std::thread::sleep;
use std::time::Duration;

use opensmith_audio::{Engine, EngineConfig, Track};
use opensmith_game::Chart;
use opensmith_session::{Command, EngineBackend, SessionConfig, SessionHandle, UiEvent, Devices};

#[test]
#[ignore = "needs audio devices"]
fn the_song_clock_follows_the_host_clock_on_real_devices() {
    let chart = Chart::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../game/tests/fixtures/demo_rock.json")).unwrap();
    let handle = SessionHandle::spawn(
        |d: Devices| Engine::start(EngineConfig { input: d.input, output: d.output, ..EngineConfig::default() }).map(EngineBackend).map_err(|e| e.to_string()),
        SessionConfig::default(),
    );
    let mut waited = 0;
    while handle.info().is_none() && handle.error().is_none() && waited < 100 {
        sleep(Duration::from_millis(50));
        waited += 1;
    }
    assert_eq!(handle.error(), None);
    let info = handle.info().expect("engine info");
    println!("audio engine up after {} ms", waited * 50);
    println!("input {} ({:?}), output {}, monitor {}", info.input_name, info.input_kind, info.output_name, info.monitor);
    let track = Track::from_interleaved(&vec![0.0; 48_000 * 2 * 40], 2);
    handle.send(Command::Volume(0.0));
    handle.send(Command::Load { chart: Box::new(chart), track: Some(track), start: -1.0, levels: Default::default() });
    handle.send(Command::Play);
    sleep(Duration::from_millis(200));
    let a = handle.poll();
    sleep(Duration::from_millis(1500));
    let b = handle.poll();
    println!("song time {:.4} → {:.4} s over 1.5 s of wall time", a.time, b.time);
    assert!(a.playing && b.playing);
    assert!((b.time - a.time - 1.5).abs() < 0.02, "clock advanced {:.4} s", b.time - a.time);
    assert!(a.events.iter().any(|e| matches!(e, UiEvent::Reset)));
    assert!(b.events.iter().chain(&a.events).any(|e| matches!(e, UiEvent::Expected { .. })));
    let stats = handle.stats().expect("stats");
    println!("round trip {:.2} ms, underruns {}, xruns {}", stats.round_trip_ms, stats.underruns, stats.errors);
}

#[test]
#[ignore = "needs audio devices"]
fn the_output_can_be_switched_on_real_devices() {
    let handle = SessionHandle::spawn(
        |d: Devices| Engine::start(EngineConfig { input: d.input, output: d.output, ..EngineConfig::default() }).map(EngineBackend).map_err(|e| e.to_string()),
        SessionConfig::default(),
    );
    let mut waited = 0;
    while handle.info().is_none() && handle.error().is_none() && waited < 100 {
        sleep(Duration::from_millis(50));
        waited += 1;
    }
    let first = handle.info().expect("engine info").output_name;
    let outputs: Vec<String> =
        opensmith_audio::list_devices().unwrap().into_iter().filter(|d| d.output_channels > 0).map(|d| d.name).collect();
    println!("outputs: {outputs:?}, playing on {first}");
    let other = outputs.iter().find(|o| **o != first).unwrap_or(&first).clone();
    handle.send(Command::Output(Some(other.clone())));
    let mut switched = false;
    for _ in 0..60 {
        sleep(Duration::from_millis(50));
        if handle.info().is_some_and(|i| i.output_name == other) || handle.switch_error().is_some() {
            switched = handle.switch_error().is_none();
            break;
        }
    }
    println!("switched to {other}: {switched} ({:?})", handle.switch_error());
    assert!(switched, "{:?}", handle.switch_error());
    handle.send(Command::Output(None));
    sleep(Duration::from_millis(800));
    assert_eq!(handle.info().unwrap().output_name, first, "back on the default output");
    assert!(handle.stats().is_some());
}
