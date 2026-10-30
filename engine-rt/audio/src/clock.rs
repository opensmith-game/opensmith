//! The host clock of CoreAudio callback timestamps, readable from any thread.

use std::sync::OnceLock;

/// Host time in nanoseconds, the clock of [`crate::PlayerClock::heard_ns`] and of input capture
/// times. On macOS this is `mach_absolute_time`, the base of cpal's CoreAudio stream instants.
#[cfg(target_os = "macos")]
pub fn host_now_ns() -> u64 {
    #[repr(C)]
    #[derive(Default)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        safe fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    static TIMEBASE: OnceLock<(u128, u128)> = OnceLock::new();
    let (numer, denom) = *TIMEBASE.get_or_init(|| {
        let mut tb = Timebase::default();
        // SAFETY: `tb` is a valid, writable mach_timebase_info_data_t.
        unsafe { mach_timebase_info(&mut tb) };
        (tb.numer.max(1) as u128, tb.denom.max(1) as u128)
    });
    (mach_absolute_time() as u128 * numer / denom) as u64
}

/// Monotonic nanoseconds since the first call (other platforms are not supported targets).
#[cfg(not(target_os = "macos"))]
pub fn host_now_ns() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}
