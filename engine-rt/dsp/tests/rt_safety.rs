//! Real-time safety: the whole per-buffer path (tracker + FX chain + chord verification) must
//! never touch the allocator, and must use a small fraction of the 64-sample buffer period.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::*;
use opensmith_dsp::synth::{mix_into, strum, Pcg32, Pluck};
use opensmith_dsp::{Algorithm, AmpKnobs, AmpModel, ChainParams, ChordConfig, ChordVerifier, GuitarChain, NoteTracker, TrackerConfig};

/// Counts allocations made by the current thread while armed (other test threads run freely).
struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

fn note_alloc() {
    if ARMED.with(Cell::get) {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note_alloc();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note_alloc();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note_alloc();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note_alloc();
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Riff + legato + strummed chords + silence: exercises every tracker state.
fn material() -> Vec<f32> {
    let mut rng = Pcg32::new(1);
    let mut x = Vec::new();
    for (k, midi) in [40, 45, 50, 55, 59, 64, 52, 52, 69, 72].into_iter().enumerate() {
        let note = common::render(Kind::Pluck, midi_freq(midi), 0.15, 0.35, k as u64);
        let at = x.len();
        mix_into(&mut x, &note, at, 1.0);
    }
    let chord: Vec<Pluck> = [40, 47, 52, 56, 59, 64]
        .iter()
        .map(|&m| {
            let mut p = Pluck::new(midi_freq(m));
            p.secs = 0.6;
            p.amp = 0.2;
            p
        })
        .collect();
    let c = strum(&chord, SR, 12.0, &mut rng);
    let at = x.len();
    mix_into(&mut x, &c, at, 1.0);
    x.extend(std::iter::repeat_n(0.0, 24_000));
    x
}

fn run_realtime_path(algorithm: Algorithm, x: &[f32]) -> (usize, usize, Vec<f64>) {
    let mut tracker = NoteTracker::new(TrackerConfig { algorithm, ..TrackerConfig::default() });
    let mut chain = GuitarChain::new(SR, ChainParams::default(), None);
    let mut verifier = ChordVerifier::new(ChordConfig::default());
    let expected = [40, 47, 52, 56, 59, 64];
    let mut buf = [0f32; BLOCK];
    let mut times = Vec::with_capacity(x.len() / BLOCK + 1);
    let mut events = 0usize;

    let before = ALLOCATIONS.load(Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    for (k, chunk) in x.chunks(BLOCK).enumerate() {
        let t = std::time::Instant::now();
        tracker.process(chunk, |_| events += 1);
        let out = &mut buf[..chunk.len()];
        out.copy_from_slice(chunk);
        chain.process(out);
        if k % 16 == 15 {
            let end = (k + 1) * BLOCK;
            let r = verifier.verify(&x[end.saturating_sub(4096)..end.min(x.len())], &expected, 440.0);
            events += r.accepted as usize;
        }
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    ARMED.with(|a| a.set(false));
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    (allocations, events, times)
}

#[test]
fn turning_the_amp_knobs_never_allocates() {
    let x = material();
    let mut chain = GuitarChain::new(SR, ChainParams::default(), None);
    let mut buf = [0f32; BLOCK];
    let models = [AmpModel::Clean, AmpModel::Crunch, AmpModel::Lead, AmpModel::Metal];
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    for (k, chunk) in x.chunks(BLOCK).enumerate() {
        if k % 8 == 0 {
            let knobs = AmpKnobs { gain: (k % 7) as f32 / 6.0, delay: 0.5, reverb: 0.5, ..AmpKnobs::default() };
            chain.set_params(ChainParams::default().with_amp(models[(k / 8) % 4], knobs));
        }
        let out = &mut buf[..chunk.len()];
        out.copy_from_slice(chunk);
        chain.process(out);
    }
    ARMED.with(|a| a.set(false));
    assert_eq!(ALLOCATIONS.load(Ordering::Relaxed) - before, 0);
}

#[test]
fn audio_path_never_allocates() {
    let x = material();
    for algorithm in [Algorithm::Mpm, Algorithm::Yin] {
        let (allocations, events, _) = run_realtime_path(algorithm, &x);
        assert!(events > 10, "{algorithm:?}: the material must exercise the tracker");
        assert_eq!(allocations, 0, "{algorithm:?}: {allocations} allocations in the audio path");
    }
}

#[test]
fn audio_path_fits_in_the_buffer_budget() {
    let x = material();
    let (_, _, mut times) = run_realtime_path(Algorithm::Mpm, &x);
    times.sort_by(f64::total_cmp);
    let budget = BLOCK as f64 / SR as f64 * 1e6;
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let p999 = times[(times.len() as f64 * 0.999) as usize];
    println!(
        "tracker + FX chain (+ chord check every 16 buffers): mean {mean:.1} µs, p99.9 {p999:.1} µs, budget {budget:.0} µs ({:.1} % mean load)",
        100.0 * mean / budget
    );
    assert!(mean < 0.15 * budget, "mean {mean:.1} µs");
    // The rare slow buffer measures the machine as much as the code: shared CI runners get
    // preempted, so there only the mean counts.
    if std::env::var_os("CI").is_none() {
        assert!(p999 < 0.6 * budget, "p99.9 {p999:.1} µs");
    }
}
