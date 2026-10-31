//! The part of a device's latency that cpal's timestamps leave out: the latency of its streams
//! (`kAudioStreamPropertyLatency`). cpal counts the device latency and the safety offset only;
//! the MacBook's speakers add 14 ms in their output stream, Bluetooth headphones far more, and
//! browsers count it. Left out, the song clock runs ahead of what is heard and the notes come
//! before their music.

use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

/// Stream latencies (ns) of the devices in use, read by the callbacks, refreshed by a thread
/// (the default output can change under a running stream: headphones plugged in).
#[derive(Debug, Default)]
pub struct StreamLatency {
    pub input_ns: AtomicU64,
    pub output_ns: AtomicU64,
}

impl StreamLatency {
    /// Reads them again: input device `input`, output device `output` (`None`: the default one).
    pub fn refresh(&self, input: &str, output: Option<&str>) {
        self.input_ns.store(stream_latency_ns(Some(input), false), Relaxed);
        self.output_ns.store(stream_latency_ns(output, true), Relaxed);
    }
}

/// Latency (ns) of the first input or output stream of the device named `name` (else the
/// default device); 0 when unknown.
#[cfg(target_os = "macos")]
pub fn stream_latency_ns(name: Option<&str>, output: bool) -> u64 {
    mac::stream_latency_ns(name, output).unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
pub fn stream_latency_ns(_name: Option<&str>, _output: bool) -> u64 {
    0
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void, CStr};
    use std::mem::size_of;
    use std::ptr::null;

    const SYSTEM_OBJECT: u32 = 1;
    const DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");
    const DEFAULT_INPUT: u32 = u32::from_be_bytes(*b"dIn ");
    const DEVICES: u32 = u32::from_be_bytes(*b"dev#");
    const NAME: u32 = u32::from_be_bytes(*b"lnam");
    const STREAMS: u32 = u32::from_be_bytes(*b"stm#");
    const LATENCY: u32 = u32::from_be_bytes(*b"ltnc");
    const SAMPLE_RATE: u32 = u32::from_be_bytes(*b"nsrt");
    const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const INPUT: u32 = u32::from_be_bytes(*b"inpt");
    const OUTPUT: u32 = u32::from_be_bytes(*b"outp");
    const UTF8: u32 = 0x0800_0100;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(id: u32, address: *const Address, qualifier_size: u32, qualifier: *const c_void, size: *mut u32) -> i32;
        fn AudioObjectGetPropertyData(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringGetCString(string: *const c_void, buffer: *mut c_char, size: isize, encoding: u32) -> u8;
        fn CFRelease(object: *const c_void);
    }

    /// A fixed-size property of object `id`.
    fn get<T: Copy + Default>(id: u32, selector: u32, scope: u32) -> Option<T> {
        let address = Address { selector, scope, element: 0 };
        let mut value = T::default();
        let mut size = size_of::<T>() as u32;
        // SAFETY: `value` is a writable `T` of `size` bytes; CoreAudio writes at most that.
        let status = unsafe { AudioObjectGetPropertyData(id, &address, 0, null(), &mut size, (&mut value as *mut T).cast()) };
        (status == 0 && size as usize == size_of::<T>()).then_some(value)
    }

    /// An array property of object `id` (device or stream ids).
    fn ids(id: u32, selector: u32, scope: u32) -> Vec<u32> {
        let address = Address { selector, scope, element: 0 };
        let mut size = 0u32;
        // SAFETY: only the size is written.
        if unsafe { AudioObjectGetPropertyDataSize(id, &address, 0, null(), &mut size) } != 0 {
            return Vec::new();
        }
        let mut out = vec![0u32; size as usize / size_of::<u32>()];
        // SAFETY: `out` holds `size` bytes.
        let status = unsafe { AudioObjectGetPropertyData(id, &address, 0, null(), &mut size, out.as_mut_ptr().cast()) };
        if status != 0 {
            return Vec::new();
        }
        out.truncate(size as usize / size_of::<u32>());
        out
    }

    fn name(device: u32) -> Option<String> {
        let string: usize = get(device, NAME, GLOBAL)?;
        let string = string as *const c_void;
        if string.is_null() {
            return None;
        }
        let mut buffer = [0 as c_char; 256];
        // SAFETY: `string` is the CFString CoreAudio returned (ours to release); the buffer is
        // 256 bytes and CFStringGetCString writes a terminated string into it on success.
        let ok = unsafe { CFStringGetCString(string, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) } != 0;
        // SAFETY: the property hands over a retained CFString.
        unsafe { CFRelease(string) };
        // SAFETY: on success the buffer holds a NUL-terminated string.
        ok.then(|| unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned())
    }

    pub fn stream_latency_ns(name_wanted: Option<&str>, output: bool) -> Option<u64> {
        let scope = if output { OUTPUT } else { INPUT };
        let device = match name_wanted {
            Some(wanted) => ids(SYSTEM_OBJECT, DEVICES, GLOBAL)
                .into_iter()
                .find(|&d| !ids(d, STREAMS, scope).is_empty() && name(d).as_deref() == Some(wanted))?,
            None => get::<u32>(SYSTEM_OBJECT, if output { DEFAULT_OUTPUT } else { DEFAULT_INPUT }, GLOBAL)?,
        };
        let stream = *ids(device, STREAMS, scope).first()?;
        let frames: u32 = get(stream, LATENCY, GLOBAL)?;
        let rate: f64 = get(device, SAMPLE_RATE, GLOBAL)?;
        (rate > 0.0).then(|| (frames as f64 / rate * 1e9) as u64)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn the_default_output_stream_latency_is_read() {
        let ns = stream_latency_ns(None, true);
        println!("default output stream latency {:.1} ms", ns as f64 * 1e-6);
        assert!(ns < 1_000_000_000, "{ns} ns");
        assert_eq!(stream_latency_ns(Some("no such device"), true), 0);
        // Devices are found under the names cpal gives them (what the engine passes).
        use cpal::traits::HostTrait;
        if let Some(device) = cpal::default_host().default_output_device() {
            assert_eq!(stream_latency_ns(Some(&device.to_string()), true), ns, "{device}");
        }
    }
}
