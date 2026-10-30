use std::path::Path;

use opensmith_audio::Track;
use opensmith_dsp::fx::resample::resample;

#[derive(Debug, thiserror::Error)]
pub enum TrackError {
    #[error("{0}: {1}")]
    Wav(String, #[source] hound::Error),
}

/// Decodes a WAV backing track to stereo at `sample_rate` (resampled if needed).
pub fn load_track(path: &Path, sample_rate: u32) -> Result<Track, TrackError> {
    let err = |e: hound::Error| TrackError::Wav(path.display().to_string(), e);
    let mut reader = hound::WavReader::open(path).map_err(err)?;
    let spec = reader.spec();
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>().map_err(err)?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1u64 << (spec.bits_per_sample.clamp(1, 32) - 1)) as f32;
            reader.samples::<i32>().map(|s| s.map(|v| v as f32 * scale)).collect::<Result<_, _>>().map_err(err)?
        }
    };
    let channels = spec.channels.max(1) as usize;
    if spec.sample_rate == sample_rate {
        return Ok(Track::from_interleaved(&data, channels));
    }
    let kept = channels.min(2);
    let resampled: Vec<Vec<f32>> = (0..kept)
        .map(|c| {
            let x: Vec<f32> = data.iter().skip(c).step_by(channels).copied().collect();
            resample(&x, spec.sample_rate as f32, sample_rate as f32, 16)
        })
        .collect();
    let frames = resampled.iter().map(Vec::len).min().unwrap_or(0);
    let interleaved: Vec<f32> = (0..frames).flat_map(|i| resampled.iter().map(move |ch| ch[i])).collect();
    Ok(Track::from_interleaved(&interleaved, kept))
}
