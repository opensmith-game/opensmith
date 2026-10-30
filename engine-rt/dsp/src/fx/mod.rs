//! Guitar effect chain: noise gate → overdrive → tone stack → cabinet IR → delay → reverb
//! (→ NAM amp model, later), set up as amp models by `amp`.

pub mod amp;
pub mod biquad;
pub mod cabinet;
pub mod chain;
pub mod drive;
pub mod gate;
pub mod resample;
pub mod space;

/// One-pole smoothing coefficient for a time constant in milliseconds.
#[inline]
pub(crate) fn time_coef(ms: f32, sample_rate: f32) -> f32 {
    if ms <= 0.0 {
        1.0
    } else {
        1.0 - (-1.0 / (ms * 1e-3 * sample_rate)).exp()
    }
}
