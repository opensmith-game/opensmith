//! CoreAudio device discovery and selection.

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::SupportedBufferSize;

use crate::AudioError;

/// Name fragments of the Real Tone Cable as reported by macOS
/// ("Rocksmith USB Guitar Adapter").
const ROCKSMITH_PATTERNS: [&str; 3] = ["rocksmith", "real tone", "guitar adapter"];

/// Whether a device name designates the Real Tone Cable.
pub fn is_rocksmith_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    ROCKSMITH_PATTERNS.iter().any(|p| lower.contains(p))
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub manufacturer: Option<String>,
    pub interface: String,
    pub input_channels: u16,
    pub output_channels: u16,
    /// Supported sample-rate ranges (min, max), input side then output side.
    pub input_rates: Vec<(u32, u32)>,
    pub output_rates: Vec<(u32, u32)>,
    /// Hardware I/O buffer range in frames.
    pub buffer_frames: Option<(u32, u32)>,
    pub default_input: bool,
    pub default_output: bool,
    pub rocksmith: bool,
}

impl DeviceInfo {
    pub fn supports_rate(&self, rate: u32, input: bool) -> bool {
        let ranges = if input { &self.input_rates } else { &self.output_rates };
        ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&rate))
    }
}

/// How the input device was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// Real Tone Cable, found automatically or chosen by name.
    Rocksmith,
    /// Device requested by name.
    Requested,
    /// No cable: system default input.
    DefaultFallback,
}

fn name_of(device: &cpal::Device) -> String {
    device.to_string()
}

/// Every CoreAudio device with its capabilities.
pub fn list_devices() -> Result<Vec<DeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let default_in = host.default_input_device().map(|d| name_of(&d));
    let default_out = host.default_output_device().map(|d| name_of(&d));
    let devices = host.devices().map_err(AudioError::backend("enumerating devices"))?;
    let mut out = Vec::new();
    for d in devices {
        let name = name_of(&d);
        let desc = d.description().ok();
        let mut info = DeviceInfo {
            rocksmith: is_rocksmith_name(&name),
            default_input: default_in.as_deref() == Some(name.as_str()),
            default_output: default_out.as_deref() == Some(name.as_str()),
            manufacturer: desc.as_ref().and_then(|x| x.manufacturer().map(str::to_owned)),
            interface: desc.as_ref().map_or_else(|| "?".into(), |x| format!("{:?}", x.interface_type())),
            name,
            input_channels: 0,
            output_channels: 0,
            input_rates: Vec::new(),
            output_rates: Vec::new(),
            buffer_frames: None,
        };
        if let Ok(configs) = d.supported_input_configs() {
            for c in configs {
                info.input_channels = info.input_channels.max(c.channels());
                info.input_rates.push((c.min_sample_rate(), c.max_sample_rate()));
                if let SupportedBufferSize::Range { min, max } = c.buffer_size() {
                    info.buffer_frames = Some((*min, *max));
                }
            }
        }
        if let Ok(configs) = d.supported_output_configs() {
            for c in configs {
                info.output_channels = info.output_channels.max(c.channels());
                info.output_rates.push((c.min_sample_rate(), c.max_sample_rate()));
                if let SupportedBufferSize::Range { min, max } = c.buffer_size() {
                    info.buffer_frames.get_or_insert((*min, *max));
                }
            }
        }
        info.input_rates.dedup();
        info.output_rates.dedup();
        out.push(info);
    }
    Ok(out)
}

/// Input selection: explicit name fragment, else the USB guitar cable, else the default input.
pub fn pick_input(host: &cpal::Host, wanted: Option<&str>) -> Result<(cpal::Device, InputKind), AudioError> {
    let mut inputs = host.input_devices().map_err(AudioError::backend("enumerating inputs"))?;
    if let Some(pattern) = wanted {
        let p = pattern.to_lowercase();
        // The cable chosen by name is still the cable (its sound goes back to the speakers).
        return inputs
            .find(|d| name_of(d).to_lowercase().contains(&p))
            .map(|d| {
                let kind = if is_rocksmith_name(&name_of(&d)) { InputKind::Rocksmith } else { InputKind::Requested };
                (d, kind)
            })
            .ok_or(AudioError::NotFound { direction: "input", pattern: pattern.into() });
    }
    if let Some(d) = inputs.find(|d| is_rocksmith_name(&name_of(d))) {
        return Ok((d, InputKind::Rocksmith));
    }
    host.default_input_device().map(|d| (d, InputKind::DefaultFallback)).ok_or(AudioError::NoInput)
}

/// Output selection: explicit name fragment, else the default output.
pub fn pick_output(host: &cpal::Host, wanted: Option<&str>) -> Result<cpal::Device, AudioError> {
    if let Some(pattern) = wanted {
        let p = pattern.to_lowercase();
        let mut outputs = host.output_devices().map_err(AudioError::backend("enumerating outputs"))?;
        return outputs
            .find(|d| name_of(d).to_lowercase().contains(&p))
            .ok_or(AudioError::NotFound { direction: "output", pattern: pattern.into() });
    }
    host.default_output_device().ok_or(AudioError::NoOutput)
}

#[cfg(test)]
mod tests {
    use super::is_rocksmith_name;

    #[test]
    fn recognises_the_real_tone_cable() {
        assert!(is_rocksmith_name("Rocksmith USB Guitar Adapter"));
        assert!(is_rocksmith_name("ROCKSMITH GUITAR ADAPTER"));
        assert!(is_rocksmith_name("Real Tone Cable"));
        assert!(!is_rocksmith_name("Micro MacBook Pro"));
        assert!(!is_rocksmith_name("Scarlett 2i2 USB"));
    }
}
