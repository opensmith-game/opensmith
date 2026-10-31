//! The host clock of the audio callbacks' timestamps, readable from any thread.

#[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
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

/// On Windows this is QueryPerformanceCounter, the base of cpal's WASAPI stream instants, on
/// the same 100 ns grid.
#[cfg(windows)]
pub fn host_now_ns() -> u64 {
    unsafe extern "system" {
        fn QueryPerformanceCounter(count: *mut i64) -> i32;
        fn QueryPerformanceFrequency(frequency: *mut i64) -> i32;
    }
    static FREQUENCY: OnceLock<u128> = OnceLock::new();
    let frequency = *FREQUENCY.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: `f` is a valid, writable LARGE_INTEGER; this never fails since Windows XP.
        unsafe { QueryPerformanceFrequency(&mut f) };
        f.max(1) as u128
    });
    let mut count = 0i64;
    // SAFETY: as above.
    unsafe { QueryPerformanceCounter(&mut count) };
    (count.max(0) as u128 * 10_000_000 / frequency * 100) as u64
}

/// On Linux this is CLOCK_MONOTONIC, the base of cpal's PipeWire stream instants. cpal's ALSA
/// and PulseAudio instants count from each stream's creation instead: not this clock.
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
pub fn host_now_ns() -> u64 {
    #[repr(C)]
    struct Timespec {
        sec: i64,
        nsec: i64,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, time: *mut Timespec) -> i32;
    }
    const CLOCK_MONOTONIC: i32 = 1;
    let mut t = Timespec { sec: 0, nsec: 0 };
    // SAFETY: `t` is a valid, writable timespec (64-bit targets: 64-bit time_t and long).
    unsafe { clock_gettime(CLOCK_MONOTONIC, &mut t) };
    t.sec as u64 * 1_000_000_000 + t.nsec as u64
}

/// Monotonic nanoseconds since the first call (other platforms are not supported targets).
#[cfg(not(any(target_os = "macos", windows, all(target_os = "linux", target_pointer_width = "64"))))]
pub fn host_now_ns() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}
