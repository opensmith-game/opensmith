//! Takes: what the guitar played during a song, with the song time of its samples, kept to
//! replay it offline through the same engine (`replay`) and measure the judging on real playing.
//!
//! A take is a folder: `guitar.wav` (mono, 32-bit float, the input as the tracker hears it, after
//! the input gain), `chart.json` (the chart as played) and `take.json` (where the samples sit in
//! the song, the latency compensation, the level of each phrase iteration, the live score).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use opensmith_game::{Chart, Score};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum TakeError {
    #[error("{0}: {1}")]
    Io(String, #[source] std::io::Error),
    #[error("{0}: {1}")]
    Json(String, #[source] serde_json::Error),
    #[error("{0}: {1}")]
    Wav(String, #[source] hound::Error),
}

/// What the live game counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakeScore {
    pub hits: u32,
    pub perfects: u32,
    pub misses: u32,
    pub extras: u32,
}

impl From<Score> for TakeScore {
    fn from(s: Score) -> Self {
        TakeScore { hits: s.hits, perfects: s.perfects, misses: s.misses, extras: s.extras }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    version: u32,
    title: String,
    sample_rate: u32,
    anchors: Vec<(usize, f64)>,
    input_offset: f64,
    levels: Vec<Option<u32>>,
    score: TakeScore,
}

pub struct Take {
    pub chart: Chart,
    pub sample_rate: u32,
    /// The guitar as the tracker heard it (mono, after the input gain).
    pub guitar: Vec<f32>,
    /// (sample index, song time of that sample) about once a second: where the take sits in the
    /// song, the input clock's drift included.
    pub anchors: Vec<(usize, f64)>,
    /// Latency compensation of the live game (s).
    pub input_offset: f64,
    /// Level each phrase iteration was played at (`None`: not reached).
    pub levels: Vec<Option<u32>>,
    pub score: TakeScore,
}

impl Take {
    pub fn seconds(&self) -> f64 {
        self.guitar.len() as f64 / self.sample_rate as f64
    }

    /// Song times the take covers.
    pub fn span(&self) -> (f64, f64) {
        let first = self.anchors.first().map_or(0.0, |a| a.1 - a.0 as f64 / self.sample_rate as f64);
        let last = self.anchors.last().map_or(0.0, |a| a.1 + (self.guitar.len() - a.0) as f64 / self.sample_rate as f64);
        (first, last)
    }

    /// A new folder of `base` for the take, named after the time and the song.
    pub fn folder_in(&self, base: &Path) -> PathBuf {
        let name: String = self.chart.meta.title.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' }).take(60).collect();
        base.join(format!("{} {}", stamp(SystemTime::now()), name.trim()))
    }

    pub fn save(&self, dir: &Path) -> Result<(), TakeError> {
        let io = |e| TakeError::Io(dir.display().to_string(), e);
        fs::create_dir_all(dir).map_err(io)?;
        let wav = dir.join("guitar.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: self.sample_rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let werr = |e| TakeError::Wav(wav.display().to_string(), e);
        let mut w = hound::WavWriter::create(&wav, spec).map_err(werr)?;
        for &v in &self.guitar {
            w.write_sample(v).map_err(werr)?;
        }
        w.finalize().map_err(werr)?;
        let json = |name: &str, text: Result<String, serde_json::Error>| -> Result<(), TakeError> {
            let path = dir.join(name);
            let text = text.map_err(|e| TakeError::Json(path.display().to_string(), e))?;
            fs::write(&path, text).map_err(|e| TakeError::Io(path.display().to_string(), e))
        };
        json("chart.json", serde_json::to_string(&self.chart))?;
        let meta = Meta {
            version: 1,
            title: self.chart.meta.title.clone(),
            sample_rate: self.sample_rate,
            anchors: self.anchors.clone(),
            input_offset: self.input_offset,
            levels: self.levels.clone(),
            score: self.score,
        };
        json("take.json", serde_json::to_string_pretty(&meta))
    }

    pub fn load(dir: &Path) -> Result<Take, TakeError> {
        let read = |name: &str| {
            let path = dir.join(name);
            fs::read_to_string(&path).map_err(|e| TakeError::Io(path.display().to_string(), e))
        };
        let meta: Meta = serde_json::from_str(&read("take.json")?).map_err(|e| TakeError::Json(dir.display().to_string(), e))?;
        let chart: Chart = serde_json::from_str(&read("chart.json")?).map_err(|e| TakeError::Json(dir.display().to_string(), e))?;
        let wav = dir.join("guitar.wav");
        let werr = |e| TakeError::Wav(wav.display().to_string(), e);
        let mut r = hound::WavReader::open(&wav).map_err(werr)?;
        let guitar = r.samples::<f32>().collect::<Result<Vec<f32>, _>>().map_err(werr)?;
        Ok(Take {
            chart,
            sample_rate: meta.sample_rate,
            guitar,
            anchors: meta.anchors,
            input_offset: meta.input_offset,
            levels: meta.levels,
            score: meta.score,
        })
    }
}

/// A take being recorded: blocks of the input tap and their song time.
#[derive(Default)]
pub(crate) struct Recording {
    pub guitar: Vec<f32>,
    pub anchors: Vec<(usize, f64)>,
    /// Blocks drained from the tap, waiting for the song clock to place them.
    pub pending: Vec<(u64, Vec<f32>)>,
}

impl Recording {
    /// Adds a block whose first sample was captured at song time `t`; an anchor about once a
    /// second.
    pub fn add(&mut self, t: f64, samples: &[f32], sample_rate: f64) {
        if self.anchors.last().is_none_or(|a| (self.guitar.len() - a.0) as f64 >= sample_rate) {
            self.anchors.push((self.guitar.len(), t));
        }
        self.guitar.extend_from_slice(samples);
    }
}

/// `2026-10-31 17h05m09s` (UTC): takes sort by when they were played.
fn stamp(t: SystemTime) -> String {
    let secs = t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + (month <= 2) as i64;
    format!("{year:04}-{month:02}-{day:02} {:02}h{:02}m{:02}s", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn takes_are_named_by_date() {
        assert_eq!(stamp(UNIX_EPOCH), "1970-01-01 00h00m00s");
        assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(1_793_466_309)), "2026-10-31 17h05m09s");
        assert_eq!(stamp(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29 00h00m00s");
    }
}
