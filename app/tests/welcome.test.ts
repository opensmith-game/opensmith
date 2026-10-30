import { describe, expect, it } from 'vitest';
import type { AudioStatus } from '../src/engine/tauri';
import { cableLine } from '../src/welcome';

const status = (over: Partial<AudioStatus>): AudioStatus =>
  ({ ready: true, error: null, rocksmith: true, input: 'Rocksmith USB Guitar Adapter', output: 'MacBook Pro Speakers', ...over }) as AudioStatus;

describe('first-launch guide', () => {
  it('says whether the USB guitar cable is there', () => {
    expect(cableLine(null)).toEqual({ ok: null, text: 'Le moteur audio démarre…' });
    expect(cableLine(status({}))).toEqual({ ok: true, text: 'Câble guitare USB branché : Rocksmith USB Guitar Adapter → MacBook Pro Speakers.' });
    const mic = cableLine(status({ rocksmith: false, input: 'Micro MacBook Pro' }));
    expect(mic.ok).toBe(false);
    expect(mic.text).toContain('« Micro MacBook Pro »');
    expect(cableLine(status({ ready: false, error: 'aucune entrée audio' }))).toEqual({ ok: false, text: 'Audio indisponible : aucune entrée audio' });
  });
});
