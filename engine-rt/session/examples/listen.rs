//! The virtual guitarist playing a chart, to listen to: a take (guitar.wav, chart.json, take.json)
//! and truth.txt (what was really played, item by item).
//!
//! cargo run --release -p opensmith-session --example listen -- <chart> <clean|good|beginner> <takes of recorded notes|synth> <out folder>
use std::path::Path;
use opensmith_game::Chart;
use opensmith_session::guitarist::{perform_with, Player};
use opensmith_session::samples::SampleBank;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (chart, player, bank, out) = (&a[1], &a[2], &a[3], &a[4]);
    let chart = Chart::from_json(&std::fs::read_to_string(chart).unwrap()).unwrap();
    let player = Player::by_name(player).unwrap();
    let bank = (bank != "synth").then(|| SampleBank::from_takes(Path::new(bank)).unwrap());
    let level = chart.arrangement.max_difficulty;
    let p = perform_with(&chart, level, &player, 7, bank.as_ref().map(|b| b as &dyn opensmith_dsp::guitar::Samples));
    p.take.save(Path::new(out)).unwrap();
    std::fs::write(Path::new(out).join("truth.txt"), p.truth.iter().map(|t| format!("{} {} {:?}\n", t.0, t.1, t.2)).collect::<String>()).unwrap();
    println!("{} s", p.take.seconds());
}
