import { describe, expect, it } from 'vitest';
import { ClockSync } from '../src/clock';
import { mulberry32 } from '../src/game/local';

describe('song clock', () => {
  it('extrapolates smoothly between jittery engine samples', () => {
    const clock = new ClockSync();
    const rand = mulberry32(3);
    let worst = 0;
    for (let ms = 0; ms < 10_000; ms += 16.7) {
      const jitter = (rand() - 0.5) * 0.008; // ±4 ms of IPC delay
      clock.sample(ms, 12 + ms / 1000 + jitter, true);
      if (ms > 1000) worst = Math.max(worst, Math.abs(clock.at(ms + 8) - (12 + (ms + 8) / 1000)));
    }
    expect(worst).toBeLessThan(0.003);
  });

  it('follows an audio clock that drifts from the system clock', () => {
    const clock = new ClockSync();
    let error = 0;
    for (let ms = 0; ms < 60_000; ms += 16.7) {
      const truth = (ms / 1000) * 1.0002; // 200 ppm fast
      clock.sample(ms, truth, true);
      error = Math.abs(clock.at(ms + 16.7) - ((ms + 16.7) / 1000) * 1.0002);
    }
    expect(error).toBeLessThan(0.001);
  });

  it('jumps on seeks and stands still while paused', () => {
    const clock = new ClockSync();
    clock.sample(0, 1, true);
    clock.sample(100, 1.1, true);
    clock.sample(200, 30, true);
    expect(clock.at(200)).toBeCloseTo(30);
    clock.sample(300, 30.1, false);
    expect(clock.at(5000)).toBeCloseTo(30.1);
  });
});
