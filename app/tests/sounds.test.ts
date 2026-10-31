import { describe, expect, it } from 'vitest';
import { LOOPS, rateOf, SOUND_NAMES, synthesize, type SoundName } from '../src/sounds';

const made = new Map<SoundName, Float32Array>(SOUND_NAMES.map((n) => [n, synthesize(n, rateOf(n))]));
const seconds = (n: SoundName) => made.get(n)!.length / rateOf(n);
const peak = (x: Float32Array) => x.reduce((m, v) => Math.max(m, Math.abs(v)), 0);
/** Root mean square of `x[from..to]`. */
const rms = (x: Float32Array, from = 0, to = x.length) => Math.sqrt(x.slice(from, to).reduce((s, v) => s + v * v, 0) / Math.max(1, to - from));

describe('sounds', () => {
  it('are made the same every time, never clip, and are never silent', () => {
    for (const name of SOUND_NAMES) {
      const x = made.get(name)!;
      expect(x.every(Number.isFinite), name).toBe(true);
      expect(peak(x), name).toBeLessThanOrEqual(0.75);
      expect(peak(x), name).toBeGreaterThan(0.05);
      expect(synthesize(name, rateOf(name))).toEqual(x);
    }
  });

  it('keep the interface quick and quiet: hover the softest, all under a quarter second', () => {
    for (const name of ['hover', 'click', 'back', 'tick', 'error'] as const) expect(seconds(name), name).toBeLessThan(0.25);
    expect(peak(made.get('hover')!)).toBeLessThan(peak(made.get('click')!));
    expect(seconds('hover')).toBeLessThan(seconds('click'));
  });

  it('end where they start for the loops: no click at the seam', () => {
    for (const name of LOOPS) {
      const x = made.get(name)!;
      const level = rms(x);
      // The jump across the seam is no bigger than between two neighbouring samples elsewhere.
      const steps = Array.from({ length: x.length - 1 }, (_, i) => Math.abs(x[i + 1] - x[i])).sort((a, b) => a - b);
      expect(Math.abs(x[0] - x[x.length - 1]), name).toBeLessThan(steps[Math.floor(steps.length * 0.999)]);
      // And the level holds: the first and last seconds as loud as the middle, give or take.
      const r = rateOf(name);
      for (const part of [rms(x, 0, r), rms(x, x.length - r)]) {
        expect(part / level, name).toBeGreaterThan(0.6);
        expect(part / level, name).toBeLessThan(1.6);
      }
    }
  });

  it('clap in time when the crowd calls for one more: two claps a second, four seconds round', () => {
    const x = made.get('clapping')!;
    const r = rateOf('clapping');
    expect(seconds('clapping')).toBeCloseTo(4, 2);
    // Energy right after each half second, against halfway between claps.
    const on = [...Array(8).keys()].map((k) => rms(x, Math.round((k / 2 + 0.005) * r), Math.round((k / 2 + 0.03) * r)));
    const off = [...Array(8).keys()].map((k) => rms(x, Math.round((k / 2 + 0.2) * r), Math.round((k / 2 + 0.3) * r)));
    on.forEach((v, k) => expect(v, `clap ${k}`).toBeGreaterThan(3 * off[k]));
  });

  it('rise and fade: applause dies away, the cheer swells then calms', () => {
    const a = made.get('applause')!;
    const r = rateOf('applause');
    expect(rms(a, a.length - Math.round(0.3 * r))).toBeLessThan(0.1 * rms(a, Math.round(0.5 * r), Math.round(1.5 * r)));
    const c = made.get('cheer')!;
    expect(rms(c, 0, Math.round(0.1 * r))).toBeLessThan(rms(c, Math.round(1 * r), Math.round(2 * r)));
  });
});
