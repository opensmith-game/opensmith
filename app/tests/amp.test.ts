import { describe, expect, it } from 'vitest';
import { AMP_MODELS, DEFAULT_AMP, knobText, preset, sanitizeAmp } from '../src/amp';

describe('amp', () => {
  it('starts on the clean channel with its own knobs', () => {
    expect(DEFAULT_AMP.model).toBe('clean');
    expect(preset('metal')).toEqual({ model: 'metal', ...AMP_MODELS[3].knobs, gateDb: null });
    expect(AMP_MODELS.map((m) => m.name)).toEqual(['clean', 'crunch', 'lead', 'metal']);
  });

  it('keeps a gate threshold of its own, in range, or the channel’s', () => {
    expect(sanitizeAmp({ ...preset('crunch'), gateDb: -72 }).gateDb).toBe(-72);
    expect(sanitizeAmp({ ...preset('crunch'), gateDb: -200 }).gateDb).toBe(-90);
    expect(sanitizeAmp({ ...preset('crunch'), gateDb: Number.NaN }).gateDb).toBeNull();
    expect(sanitizeAmp(undefined).gateDb).toBeNull();
  });

  it('brings saved settings back into range', () => {
    const odd = { model: 'jazz', gain: 3, bass: -1, mid: Number.NaN, volume: 0.4, gate: 'yes' } as unknown as Parameters<typeof sanitizeAmp>[0];
    const amp = sanitizeAmp(odd);
    expect(amp.model).toBe('clean');
    expect([amp.gain, amp.bass, amp.mid, amp.volume, amp.gate]).toEqual([1, 0, preset('clean').mid, 0.4, true]);
    expect(sanitizeAmp(undefined)).toEqual(DEFAULT_AMP);
    expect(sanitizeAmp({ ...preset('lead'), gain: 0.25 })).toEqual({ ...preset('lead'), gain: 0.25 });
  });

  it('reads knobs from 0 to 10 like an amp', () => {
    expect([knobText(0), knobText(0.55), knobText(1)]).toEqual(['0,0', '5,5', '10,0']);
  });
});
