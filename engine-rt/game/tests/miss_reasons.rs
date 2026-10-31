//! A miss says why: nothing played, the right note too early or too late, another note, or a
//! strum that was not the chord.

use opensmith_game::{Chart, Game, GameConfig, Judgement, MissReason};

/// A note (low E string, fret 3: G2, MIDI 43) at 1 s, power chords at 3 s and 5 s.
fn game() -> Game {
    let json = r#"{"format":"opensmith-chart","version":1,
        "meta":{"title":"T","artist":"A","album":null,"year":null,"duration":8.0,"source":{"kind":"audio","file":"x"},"generator":"test"},
        "audio":{"backing":null,"guitar":null,"mix":null,"offset":0.0},
        "arrangement":{"name":"Lead","type":"lead","tuning":{"name":"E Standard","strings":[40,45,50,55,59,64]},"capo":0,"maxDifficulty":0},
        "tempo":{"bpm":120.0,"timeSignature":[4,4]},
        "beats":[{"time":0.0,"bar":1}],"sections":[],
        "phrases":[{"name":"A","start":0.0,"end":8.0,"maxDifficulty":0}],
        "chordTemplates":[{"name":"E5","frets":[0,2,2,-1,-1,-1],"fingers":[0,1,1,-1,-1,-1]}],
        "notes":[{"time":1.0,"string":0,"fret":3,"sustain":0.0,"levels":[0,0]}],
        "chords":[{"time":3.0,"template":0,"sustain":0.0,"levels":[0,0]},{"time":5.0,"template":0,"sustain":0.0,"levels":[0,0]}],
        "anchors":[],"handShapes":[]}"#;
    Game::new(Chart::from_json(json).unwrap(), GameConfig { dynamic_difficulty: false, ..GameConfig::default() })
}

/// Reason of each miss by item, and the late explanations (item, error).
type Explained = (Vec<(usize, MissReason)>, Vec<(usize, f64)>);

/// Runs the clock to 8 s, calling `play(game, now)` every 10 ms.
fn run(mut play: impl FnMut(&mut Game, f64) -> Vec<Judgement>) -> Explained {
    let mut g = game();
    let mut out = Vec::new();
    let mut now = 0.0;
    while now < 8.0 {
        now += 0.01;
        out.extend(g.advance(now));
        out.extend(play(&mut g, now));
    }
    let misses = out.iter().filter_map(|j| if let Judgement::Miss { index, reason, .. } = j { Some((*index, *reason)) } else { None }).collect();
    let late = out.iter().filter_map(|j| if let Judgement::Late { index, error } = j { Some((*index, *error)) } else { None }).collect();
    (misses, late)
}

/// Plays `midi` once, at song time `at`.
fn note_at(at: f64, midi: i32) -> impl FnMut(&mut Game, f64) -> Vec<Judgement> {
    let mut done = false;
    move |g, now| {
        if done || now < at {
            return vec![];
        }
        done = true;
        g.on_note(at, midi).into_iter().collect()
    }
}

fn timing(reason: MissReason) -> f64 {
    match reason {
        MissReason::Timing { error } => error,
        other => panic!("expected a timing reason, got {other:?}"),
    }
}

#[test]
fn nothing_played_is_unplayed() {
    let (misses, late) = run(|_, _| vec![]);
    assert_eq!(misses, vec![(0, MissReason::Unplayed), (1, MissReason::Unplayed), (2, MissReason::Unplayed)]);
    assert!(late.is_empty());
}

#[test]
fn the_right_note_too_early() {
    let (misses, _) = run(note_at(0.8, 43));
    assert!((timing(misses[0].1) + 0.2).abs() < 1e-9, "{misses:?}");
}

#[test]
fn the_right_note_too_late_before_the_miss_is_called() {
    let (misses, late) = run(note_at(1.14, 43));
    assert!((timing(misses[0].1) - 0.14).abs() < 1e-9, "{misses:?}");
    assert!(late.is_empty());
}

#[test]
fn the_right_note_too_late_after_the_miss_explains_it_then() {
    let (misses, late) = run(note_at(1.3, 43));
    assert_eq!(misses[0], (0, MissReason::Unplayed));
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].0, 0);
    assert!((late[0].1 - 0.3).abs() < 1e-9);
}

#[test]
fn far_too_late_explains_nothing() {
    let (_, late) = run(note_at(1.5, 43));
    assert!(late.is_empty());
}

#[test]
fn another_note_is_too_high_or_too_low() {
    let (misses, _) = run(note_at(1.02, 45));
    assert_eq!(misses[0], (0, MissReason::Pitch { semitones: 2 }));
    let (misses, _) = run(note_at(0.97, 42));
    assert_eq!(misses[0], (0, MissReason::Pitch { semitones: -1 }));
}

#[test]
fn a_strum_the_verifier_rejects_is_the_wrong_chord() {
    let (misses, _) = run(|g, now| {
        if (now - 3.2).abs() < 0.005 {
            g.on_chord(1, false, Some(3.01));
        }
        vec![]
    });
    assert_eq!(misses[1], (1, MissReason::WrongChord));
    assert_eq!(misses[2], (2, MissReason::Unplayed));
}

#[test]
fn a_chord_strummed_out_of_time() {
    // Too early: a strum 250 ms before the chord, nothing in its window.
    let mut strummed = false;
    let (misses, _) = run(|g, now| {
        if !strummed && now >= 2.75 {
            strummed = true;
            return g.on_strum(2.75).into_iter().collect();
        }
        vec![]
    });
    assert!((timing(misses[1].1) + 0.25).abs() < 1e-9, "{misses:?}");

    // Heard as the chord, but 150 ms late: the verifier's attack says so.
    let (misses, _) = run(|g, now| {
        if (now - 3.19).abs() < 0.005 {
            g.on_chord(1, true, Some(3.15));
        }
        vec![]
    });
    assert!((timing(misses[1].1) - 0.15).abs() < 1e-9, "{misses:?}");

    // A strum after the miss was called: late, said then.
    let mut strummed = false;
    let (_, late) = run(|g, now| {
        if !strummed && now >= 5.3 {
            strummed = true;
            return g.on_strum(5.3).into_iter().collect();
        }
        vec![]
    });
    assert_eq!(late.len(), 1);
    assert_eq!(late[0].0, 2);
}

#[test]
fn a_strum_for_a_note_played_well_explains_no_chord() {
    // The note at 1 s is hit; its strum is not the chord at 3 s (too far) nor anyone's excuse.
    let mut g = game();
    let mut misses = Vec::new();
    let mut now = 0.0;
    while now < 8.0 {
        now += 0.01;
        misses.extend(g.advance(now));
        if (now - 1.0).abs() < 0.005 {
            g.on_strum(1.0);
            g.on_note(1.0, 43);
        }
    }
    assert!(misses.iter().all(|j| !matches!(j, Judgement::Miss { reason: MissReason::Timing { .. }, .. })));
}
