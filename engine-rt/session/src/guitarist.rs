//! A virtual guitarist: plays a chart on the virtual guitar (`opensmith_dsp::guitar`) the way a
//! player of some level would, and keeps what was really played, every mistake included. The
//! result is a take: replayed through the engine, its judging can be checked note by note
//! against the truth, on any chart, without a guitar.

use opensmith_dsp::guitar::{render_with, Excitation, GuitarSound, Kind, Samples, Voice};
use opensmith_dsp::synth::Pcg32;
use opensmith_game::{Chart, Game, GameConfig, Target};

use crate::take::{Take, TakeScore};

/// How a player plays: timing, mistakes, how cleanly strings are damped.
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub name: &'static str,
    /// Timing error of the notes played on time: mean and spread (s).
    pub bias: f64,
    pub spread: f64,
    /// Chances, per note or chord, of: leaving it out, playing it far out of time, a wrong fret,
    /// a chord with a string missed, a chord with an open string brushed too.
    pub skip: f64,
    pub off_time: f64,
    pub wrong: f64,
    pub partial: f64,
    pub brushed: f64,
    /// Chance that a string is left ringing when the hand moves on.
    pub let_ring: f64,
    /// Each string this many cents out of tune at most.
    pub detune: f64,
    /// A beginner's hands, per note or chord played right: a string that buzzes against the
    /// fret, a finger pressing so hard the note goes sharp, a finger too light (a note: it only
    /// thuds; a chord: one string thuds), the pick scratching the strings just before, a chord
    /// strummed while the fingers are still on their way from the last one.
    pub buzz: f64,
    pub sharp: f64,
    pub dead: f64,
    pub scratch: f64,
    pub unformed: f64,
}

impl Player {
    /// Every note right, on time give or take a few ms, strings damped as they should.
    pub fn clean() -> Player {
        Player { name: "clean", bias: 0.0, spread: 0.006, skip: 0.0, off_time: 0.0, wrong: 0.0, partial: 0.0, brushed: 0.0, let_ring: 0.1, detune: 4.0, ..Player::HANDS_OK }
    }

    /// A good amateur: a little late, a few slips, chords sometimes short of a string.
    pub fn good() -> Player {
        Player {
            name: "good",
            bias: 0.012,
            spread: 0.022,
            skip: 0.03,
            off_time: 0.03,
            wrong: 0.04,
            partial: 0.1,
            brushed: 0.1,
            let_ring: 0.35,
            detune: 8.0,
            buzz: 0.03,
            sharp: 0.03,
            dead: 0.01,
            scratch: 0.03,
            unformed: 0.02,
        }
    }

    /// Learning: late and uneven, many notes left out or wrong, strings ringing everywhere.
    pub fn beginner() -> Player {
        Player {
            name: "beginner",
            bias: 0.03,
            spread: 0.04,
            skip: 0.15,
            off_time: 0.1,
            wrong: 0.12,
            partial: 0.25,
            brushed: 0.25,
            let_ring: 0.7,
            detune: 15.0,
            buzz: 0.15,
            sharp: 0.1,
            dead: 0.08,
            scratch: 0.12,
            unformed: 0.12,
        }
    }

    /// Hands that never buzz, press too hard or too lightly, scratch, or strum early.
    const HANDS_OK: Player = Player {
        name: "",
        bias: 0.0,
        spread: 0.0,
        skip: 0.0,
        off_time: 0.0,
        wrong: 0.0,
        partial: 0.0,
        brushed: 0.0,
        let_ring: 0.0,
        detune: 0.0,
        buzz: 0.0,
        sharp: 0.0,
        dead: 0.0,
        scratch: 0.0,
        unformed: 0.0,
    };

    pub fn by_name(name: &str) -> Option<Player> {
        [Player::clean(), Player::good(), Player::beginner()].into_iter().find(|p| p.name == name)
    }
}

/// What the player really did with an expected note or chord.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Played {
    /// As written, on time (within the player's spread).
    Clean,
    /// A chord with one string missed (four strings or more).
    Partial,
    /// A chord with an open string brushed on top.
    Brushed,
    /// The right note or chord, `error` seconds out of time (beyond the hit window).
    OffTime { error: f64 },
    /// A wrong fret: `semitones` off (a note), or one string of a chord off.
    Wrong { semitones: i32 },
    Skipped,
    /// The right note or chord, a string buzzing against the fret.
    Buzzed,
    /// The right note pressed so hard it sounds 20 to 40 cents sharp.
    Sharp,
    /// A note pressed too lightly: a thud, no pitch.
    Dead,
    /// A chord with one string pressed too lightly (a thud among the others).
    Muffled,
    /// The right note or chord, the pick scratching the strings just before.
    Scratched,
    /// A chord strummed before the fingers got there: half its strings still where the last
    /// chord had them.
    Unformed,
}

impl Played {
    /// What a fair judge says: what was played right counts, a chord short of a string or with
    /// a string brushed too included.
    pub fn deserves_hit(&self) -> bool {
        matches!(self, Played::Clean | Played::Partial | Played::Brushed | Played::Buzzed | Played::Sharp | Played::Muffled | Played::Scratched)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Played::Clean => "juste",
            Played::Partial => "accord à une corde près",
            Played::Brushed => "accord + corde à vide frôlée",
            Played::OffTime { .. } => "hors du temps",
            Played::Wrong { .. } => "fausse note",
            Played::Skipped => "pas jouée",
            Played::Buzzed => "corde qui frise",
            Played::Sharp => "un peu haute (doigt trop appuyé)",
            Played::Dead => "étouffée (doigt pas assez appuyé)",
            Played::Muffled => "accord + une corde étouffée",
            Played::Scratched => "bruit de médiator juste avant",
            Played::Unformed => "accord pas encore en place",
        }
    }
}

/// A chart played: the take, and the truth about each expected item.
pub struct Performance {
    pub take: Take,
    /// (is a chord, index in the chart's notes or chords, what was played), for each item the
    /// game expects.
    pub truth: Vec<(bool, usize, Played)>,
    /// Every note the guitar sounded, in time order, its times from the start of the take.
    pub voices: Vec<Voice>,
}

/// Seconds of guitar before the first note.
const LEAD: f64 = 2.0;
/// Open strings of the guitars the recorded notes come from (E standard).
const RECORDED_OPEN: [i32; 6] = [40, 45, 50, 55, 59, 64];

/// The pick scratching across a string or two, muted by the other hand, 60 to 150 ms before
/// `at`: a click and a thud, no pitch.
fn scratch(rng: &mut Pcg32, at: f64, strings: usize, chart: &Chart, detune: &[f64]) -> Voice {
    let s = (rng.next_u32() as usize) % strings.max(1);
    let start = at - rng.range(0.06, 0.15) as f64;
    Voice {
        string: s,
        start,
        end: start + 0.05,
        midi: chart.arrangement.tuning.strings[s] as f64 + detune[s],
        glide: Vec::new(),
        vibrato: 0.0,
        excitation: Excitation::Dead { level: rng.range(0.3, 0.6) },
        palm: false,
        recorded: None,
        buzz: 0.0,
    }
}

/// The fret of a recorded (E standard) guitar that sounds `midi` on `string`, if it has one.
fn recorded_fret(string: usize, midi: i32) -> Option<i32> {
    let fret = midi - RECORDED_OPEN.get(string)?;
    (0..=22).contains(&fret).then_some(fret)
}

/// `player` plays `chart` with every phrase at `level` (capped at the chart's top level).
pub fn perform(chart: &Chart, level: u32, player: &Player, seed: u64) -> Performance {
    perform_with(chart, level, player, seed, None)
}

/// `perform` on a guitar made of recorded notes where `samples` has them.
pub fn perform_with(chart: &Chart, level: u32, player: &Player, seed: u64, samples: Option<&dyn Samples>) -> Performance {
    let mut rng = Pcg32::new(seed);
    let mut game = Game::new(chart.clone(), GameConfig { dynamic_difficulty: false, start_level: level, ..GameConfig::default() });
    game.activate_all();
    let mut items = game.expected().to_vec();
    items.sort_by(|a, b| a.time.total_cmp(&b.time));
    let levels: Vec<Option<u32>> = (0..chart.phrases.len()).map(|k| game.iteration_level(k)).collect();
    let strings = chart.arrangement.tuning.strings.len();
    let detune: Vec<f64> = (0..strings).map(|_| player.detune / 100.0 * (2.0 * rng.uniform() as f64 - 1.0)).collect();
    let t0 = items.first().map_or(0.0, |e| e.time) - LEAD;
    let chance = |rng: &mut Pcg32, p: f64| (rng.uniform() as f64) < p;
    // The beginner's defects draw only when they can happen: the other players' notes stay as
    // they were.
    let maybe = |rng: &mut Pcg32, p: f64| p > 0.0 && (rng.uniform() as f64) < p;
    let mut last_chord: Option<Vec<i32>> = None;

    // (string, time, voice, damped when the hand moves on)
    let mut voices: Vec<(Voice, bool)> = Vec::new();
    let mut truth = Vec::with_capacity(items.len());
    for (k, e) in items.iter().enumerate() {
        let chord = matches!(e.target, Target::Chord { .. });
        let next = items.get(k + 1).map_or(e.time + 1.0, |n| n.time);
        let r = rng.uniform() as f64;
        let mut played = if r < player.skip {
            Played::Skipped
        } else if r < player.skip + player.off_time {
            let late = rng.uniform() < 0.6;
            Played::OffTime { error: if late { 1.0 } else { -1.0 } * rng.range(0.15, 0.3) as f64 }
        } else if r < player.skip + player.off_time + player.wrong {
            Played::Wrong { semitones: if rng.uniform() < 0.5 { 1 } else { -1 } * if rng.uniform() < 0.7 { 1 } else { 2 } }
        } else {
            Played::Clean
        };
        let error = match played {
            Played::OffTime { error } => error,
            _ => player.bias + player.spread * rng.gauss() as f64,
        };
        let at = e.time + error - t0;
        let level = rng.range(0.6, 1.0);
        match (&e.target, played) {
            (_, Played::Skipped) => {}
            (Target::Note { string, fret, .. }, _) => {
                let n = &chart.notes[e.event];
                let tech = n.techniques.clone().unwrap_or_default();
                let mut fret = *fret as i32;
                if let Played::Wrong { semitones } = played {
                    if tech.mute == Some(true) || tech.harmonic == Some(true) || tech.pinch_harmonic == Some(true) || fret + semitones < 0 {
                        played = Played::Clean; // no wrong pitch to play there
                    } else {
                        fret += semitones;
                    }
                }
                let s = *string as usize;
                let open = chart.arrangement.tuning.strings[s] as f64 + detune[s];
                let excitation = if tech.mute == Some(true) {
                    Excitation::Dead { level }
                } else if tech.harmonic == Some(true) || tech.pinch_harmonic == Some(true) {
                    let n = match fret {
                        12 => 2,
                        7 | 19 => 3,
                        5 | 24 => 4,
                        4 | 9 | 16 => 5,
                        _ => 6,
                    };
                    Excitation::Harmonic { level, n }
                } else if tech.hammer_on == Some(true) || tech.pull_off == Some(true) {
                    Excitation::Legato { level: level * 0.6 }
                } else {
                    Excitation::Pick { level, pos: rng.range(0.1, 0.25) }
                };
                let mut glide = Vec::new();
                if let Some(to) = tech.slide_to.filter(|&to| to as i32 != fret) {
                    glide = vec![(0.04, 0.0), ((n.sustain * 0.8).max(0.12), to as f64 - fret as f64)];
                } else if let Some(bend) = tech.bend.filter(|&b| b > 0.0) {
                    glide = vec![(0.06, 0.0), (0.2, bend as f64)];
                }
                let harmonic = matches!(excitation, Excitation::Harmonic { .. });
                let kind = if tech.pinch_harmonic == Some(true) {
                    Kind::Pinch
                } else if harmonic {
                    Kind::Harmonic
                } else if tech.palm_mute == Some(true) {
                    Kind::PalmMute
                } else if tech.vibrato == Some(true) {
                    Kind::Vibrato
                } else {
                    Kind::Plain
                };
                // A harmonic is recorded by its node, on a string tuned as the recorded ones.
                let recorded = if harmonic {
                    (open.round() as i32 == RECORDED_OPEN[s]).then(|| (fret, kind, rng.next_u32()))
                } else {
                    recorded_fret(s, open.round() as i32 + fret).map(|f| (f, kind, rng.next_u32()))
                };
                let ring = tech.palm_mute != Some(true) && (fret == 0 || harmonic || chance(&mut rng, player.let_ring));
                let end = if tech.palm_mute == Some(true) {
                    at + 0.12 + n.sustain
                } else if ring {
                    at + n.sustain.max(rng.range(1.0, 2.5) as f64)
                } else {
                    at + n.sustain.max(next - e.time + rng.range(0.02, 0.08) as f64)
                };
                let mut voice = Voice {
                    string: s,
                    start: at,
                    end,
                    midi: if harmonic { open } else { open + fret as f64 },
                    glide,
                    vibrato: if tech.vibrato == Some(true) { 0.15 } else { 0.0 },
                    excitation,
                    palm: tech.palm_mute == Some(true),
                    recorded,
                    buzz: 0.0,
                };
                let pressed = fret > 0 && matches!(voice.excitation, Excitation::Pick { .. } | Excitation::Legato { .. });
                if played == Played::Clean && pressed {
                    if maybe(&mut rng, player.buzz) {
                        voice.buzz = rng.range(0.5, 1.0);
                        played = Played::Buzzed;
                    } else if maybe(&mut rng, player.sharp) {
                        voice.midi += rng.range(0.2, 0.4) as f64;
                        voice.recorded = None; // the recordings are in tune
                        played = Played::Sharp;
                    } else if maybe(&mut rng, player.dead) {
                        voice.excitation = Excitation::Dead { level };
                        played = Played::Dead;
                    }
                }
                if played == Played::Clean && maybe(&mut rng, player.scratch) {
                    voices.push((scratch(&mut rng, at, strings, chart, &detune), true));
                    played = Played::Scratched;
                }
                voices.push((voice, !ring));
            }
            (Target::Chord { template, .. }, _) => {
                let c = &chart.chords[e.event];
                let tech = c.techniques.clone().unwrap_or_default();
                let frets = &chart.chord_templates[*template].frets;
                let mut sounding: Vec<(usize, i32)> = frets.iter().enumerate().filter(|(_, f)| **f >= 0).map(|(s, f)| (s, *f)).collect();
                if played == Played::Clean && sounding.len() >= 4 && chance(&mut rng, player.partial) {
                    let drop = 1 + (rng.next_u32() as usize) % (sounding.len() - 1); // not the bass
                    sounding.remove(drop);
                    played = Played::Partial;
                } else if played == Played::Clean && chance(&mut rng, player.brushed) {
                    // The pick crosses the string next to the chord's on its way: the one just
                    // below its lowest or just above its highest.
                    let lo = sounding.first().map_or(0, |sf| sf.0);
                    let hi = sounding.last().map_or(0, |sf| sf.0);
                    let free: Vec<usize> = [lo.checked_sub(1), Some(hi + 1)].into_iter().flatten().filter(|&s| s < strings && frets.get(s).is_none_or(|f| *f < 0)).collect();
                    if let Some(&s) = free.get((rng.next_u32() as usize) % free.len().max(1)) {
                        sounding.push((s, 0));
                        played = Played::Brushed;
                    }
                }
                if let Played::Wrong { semitones } = played {
                    let k = (rng.next_u32() as usize) % sounding.len().max(1);
                    if let Some(sf) = sounding.get_mut(k) {
                        sf.1 = (sf.1 + semitones).max(0);
                    }
                }
                // A beginner's hand: (string, thuds, buzzes) for the strings that differ.
                let mut thud: Option<usize> = None;
                let mut buzzing: Option<usize> = None;
                if played == Played::Clean {
                    let previous = last_chord.as_ref().filter(|p| p.as_slice() != frets.as_slice());
                    if let Some(prev) = previous.filter(|_| maybe(&mut rng, player.unformed)) {
                        // Half the strings, the bass first, still where the last chord had them.
                        let moved: Vec<usize> = sounding.iter().map(|sf| sf.0).filter(|&s| prev.get(s).is_some_and(|&f| f >= 0 && f != frets[s])).collect();
                        if !moved.is_empty() {
                            for sf in sounding.iter_mut().filter(|sf| moved[..moved.len().div_ceil(2)].contains(&sf.0)) {
                                sf.1 = prev[sf.0];
                            }
                            played = Played::Unformed;
                        }
                    } else if sounding.len() >= 4 && maybe(&mut rng, player.dead) {
                        thud = Some(sounding[1 + (rng.next_u32() as usize) % (sounding.len() - 1)].0);
                        played = Played::Muffled;
                    } else if maybe(&mut rng, player.buzz) {
                        let fretted: Vec<usize> = sounding.iter().filter(|sf| sf.1 > 0).map(|sf| sf.0).collect();
                        if let Some(&s) = fretted.get((rng.next_u32() as usize) % fretted.len().max(1)) {
                            buzzing = Some(s);
                            played = Played::Buzzed;
                        }
                    }
                }
                if played == Played::Clean && maybe(&mut rng, player.scratch) {
                    voices.push((scratch(&mut rng, at, strings, chart, &detune), true));
                    played = Played::Scratched;
                }
                last_chord = Some(frets.clone());
                sounding.sort_by_key(|sf| sf.0);
                if c.strum.as_deref() == Some("up") {
                    sounding.reverse();
                }
                let gap = rng.range(0.004, 0.012) as f64;
                for (j, (s, fret)) in sounding.into_iter().enumerate() {
                    let start = at + j as f64 * gap;
                    let open = chart.arrangement.tuning.strings[s] as f64 + detune[s];
                    let ring = tech.palm_mute != Some(true) && (fret == 0 || chance(&mut rng, player.let_ring));
                    let end = if tech.palm_mute == Some(true) {
                        start + 0.12 + c.sustain
                    } else if ring {
                        start + c.sustain.max(rng.range(1.0, 2.5) as f64)
                    } else {
                        start + c.sustain.max(next - e.time + rng.range(0.02, 0.08) as f64)
                    };
                    let excitation = if tech.mute == Some(true) || thud == Some(s) {
                        Excitation::Dead { level: level * 0.7 }
                    } else {
                        Excitation::Pick { level: level * 0.8, pos: rng.range(0.1, 0.25) }
                    };
                    let kind = if tech.palm_mute == Some(true) { Kind::PalmMute } else { Kind::Plain };
                    let recorded = recorded_fret(s, open.round() as i32 + fret).map(|f| (f, kind, rng.next_u32()));
                    let voice =
                        Voice { string: s, start, end, midi: open + fret as f64, glide: Vec::new(), vibrato: 0.0, excitation, palm: tech.palm_mute == Some(true), recorded, buzz: if buzzing == Some(s) { rng.range(0.5, 1.0) } else { 0.0 } };
                    voices.push((voice, !ring));
                }
            }
        }
        truth.push((chord, e.event, played));
    }
    // One note at a time per string: the next one damps it.
    voices.sort_by(|a, b| a.0.start.total_cmp(&b.0.start));
    for i in 0..voices.len() {
        let s = voices[i].0.string;
        if let Some(next) = voices[i + 1..].iter().find(|v| v.0.string == s).map(|v| v.0.start) {
            voices[i].0.end = voices[i].0.end.min(next);
        }
    }
    let voices: Vec<Voice> = voices.into_iter().map(|v| v.0).collect();
    let seconds = voices.iter().map(|v| v.end).fold(LEAD, f64::max) + 1.0;
    let sound = GuitarSound { peak: 0.2 + 0.1 * rng.uniform(), hum_hz: if rng.uniform() < 0.5 { 50.0 } else { 60.0 }, ..GuitarSound::default() };
    let guitar = render_with(&voices, seconds, &sound, seed ^ 0x9e37_79b9, samples);
    let take = Take {
        chart: chart.clone(),
        sample_rate: sound.sample_rate as u32,
        guitar,
        anchors: vec![(0, t0)],
        input_offset: 0.0,
        levels,
        score: TakeScore::default(),
    };
    Performance { take, truth, voices }
}
