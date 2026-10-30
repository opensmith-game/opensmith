//! The two CoreAudio callback bodies never touch the allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::sync::Arc;

use opensmith_audio::{Bridge, EngineConfig, PlayerCommand, Track};
use opensmith_dsp::synth::{Pcg32, Pluck};
use opensmith_dsp::{AmpKnobs, AmpModel, ChainParams};

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

#[test]
fn input_and_output_callbacks_never_allocate() {
    // Backing tracks are built (and later freed) by the control thread, which keeps a reference
    // to every track it sends.
    let first = Arc::new(Track::from_interleaved(&vec![0.1f32; 2 * 48_000], 2));
    let second = Arc::new(Track::from_interleaved(&vec![0.2f32; 2 * 48_000], 2));
    let mut bridge = Bridge::new(&EngineConfig::default(), 1, 2, 1.0);
    let mut rng = Pcg32::new(1);
    let mut signal = Vec::new();
    for midi in [40, 47, 52, 59, 64, 69] {
        let mut p = Pluck::new(opensmith_dsp::note::midi_to_freq(midi as f32, 440.0));
        p.secs = 0.3;
        signal.extend(p.render(48_000.0, &mut rng));
    }
    let mut out = [0f32; 128];
    let mut events = 0;
    let period_ns = 64.0 / 48_000.0 * 1e9;

    let before = ALLOCATIONS.load(Ordering::Relaxed);
    ARMED.with(|a| a.set(true));
    bridge.monitor.send(PlayerCommand::Load { track: Some(first.clone()), frame: -3_000, generation: 1 });
    bridge.monitor.send(PlayerCommand::Play);
    for (k, block) in signal.chunks_exact(64).enumerate() {
        match k {
            100 => drop(bridge.monitor.send(PlayerCommand::Rate(0.75))),
            150 => drop(bridge.monitor.send(PlayerCommand::Loop(Some((-2_000, 9_000))))),
            300 => drop(bridge.monitor.send(PlayerCommand::Seek { frame: 10_000, generation: 2 })),
            350 => drop(bridge.monitor.set_chain(ChainParams::default().with_amp(AmpModel::Metal, AmpKnobs { delay: 0.5, reverb: 0.5, ..AmpKnobs::default() }))),
            450 => drop(bridge.monitor.send(PlayerCommand::Rate(0.5))),
            500 => drop(bridge.monitor.send(PlayerCommand::Loop(None))),
            550 => drop(bridge.monitor.send(PlayerCommand::Rate(1.0))),
            600 => drop(bridge.monitor.send(PlayerCommand::Load { track: Some(second.clone()), frame: 0, generation: 3 })),
            601 => drop(bridge.monitor.send(PlayerCommand::Play)),
            _ => {}
        }
        let t = k as f64 * period_ns;
        bridge.input.process(block, (t + 0.5 * period_ns) as u64, 2_000_000);
        bridge.output.process(&mut out, (t + period_ns) as u64, 4_000_000);
    }
    ARMED.with(|a| a.set(false));
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(bridge.monitor.clock().generation, 3);
    assert_eq!(Arc::strong_count(&first), 1, "the callback let go of the first track");

    bridge.monitor.poll_events(|_| events += 1);
    assert!(events > 6, "the material must produce note events");
    assert_eq!(allocations, 0, "{allocations} allocations inside the callbacks");
}
