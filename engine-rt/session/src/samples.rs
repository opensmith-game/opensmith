//! Recorded notes for the virtual guitar, cut out of takes of single notes played one by one
//! (each string at each fret, plain, palm-muted, with vibrato, as pinch or natural harmonics):
//! real attacks, timbres, decays and noises, from as many guitars as there are takes.

use std::collections::HashMap;
use std::path::Path;

use opensmith_dsp::guitar::{Kind, Samples};

use crate::take::{Take, TakeError};

/// Longest recording kept per note (s).
const LONGEST: f64 = 4.0;

#[derive(Default)]
pub struct SampleBank {
    notes: HashMap<(usize, i32, Kind), Vec<Vec<f32>>>,
}

impl SampleBank {
    /// The notes of the takes in `dir` (folders holding a take) whose name says how they were
    /// played: `singlenotes`, `palmmute`, `vibrato`, `pinchharmonics`, `harmonics`. Each note of
    /// their chart is cut from its attack to its end (or the next note), and scaled to the same
    /// peak: the guitarist's dynamics come on top.
    pub fn from_takes(dir: &Path) -> Result<SampleBank, TakeError> {
        let mut bank = SampleBank::default();
        let mut folders: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| TakeError::Io(dir.display().to_string(), e))?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join("take.json").is_file())
            .collect();
        folders.sort();
        for folder in folders {
            let name = folder.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            let kind = if name.contains("singlenotes") {
                Kind::Plain
            } else if name.contains("palmmute") {
                Kind::PalmMute
            } else if name.contains("vibrato") {
                Kind::Vibrato
            } else if name.contains("pinch") {
                Kind::Pinch
            } else if name.contains("harmonics") {
                Kind::Harmonic
            } else {
                continue;
            };
            let take = Take::load(&folder)?;
            let sr = take.sample_rate as f64;
            let mut notes = take.chart.notes.clone();
            notes.sort_by(|a, b| a.time.total_cmp(&b.time));
            for (k, n) in notes.iter().enumerate() {
                let next = notes.get(k + 1).map_or(f64::INFINITY, |m| m.time);
                let from = ((n.time - 0.003) * sr).max(0.0) as usize;
                let to = (((n.time + n.sustain + 0.15).min(next - 0.01).min(n.time + LONGEST)) * sr) as usize;
                let Some(cut) = take.guitar.get(from..to.min(take.guitar.len())) else { continue };
                let peak = cut.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                if cut.len() < (0.1 * sr) as usize || peak < 1e-4 {
                    continue;
                }
                let note: Vec<f32> = cut.iter().map(|v| v * 0.5 / peak).collect();
                bank.notes.entry((n.string as usize, n.fret as i32, kind)).or_default().push(note);
            }
        }
        Ok(bank)
    }

    /// Recordings held, all kinds together.
    pub fn len(&self) -> usize {
        self.notes.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }
}

impl Samples for SampleBank {
    fn note(&self, string: usize, fret: i32, kind: Kind, choice: u32) -> Option<&[f32]> {
        // A technique without a recording at that fret falls back to the plain note there.
        let found = self.notes.get(&(string, fret, kind)).or_else(|| self.notes.get(&(string, fret, Kind::Plain)).filter(|_| kind != Kind::Harmonic))?;
        found.get(choice as usize % found.len()).map(Vec::as_slice)
    }
}
