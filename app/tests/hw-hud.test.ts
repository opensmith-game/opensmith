import { describe, expect, it } from 'vitest';
import { cableToTake } from '../src/hw-hud';

const CABLE = { name: 'Rocksmith USB Guitar Adapter', rocksmith: true };
const MIC = { name: 'Micro MacBook Pro', rocksmith: false };
const onMic = { ready: true, error: null, rocksmith: false };

describe('the USB guitar cable plugged in while the app runs', () => {
  it('is taken at once instead of the Mac microphone', () => {
    expect(cableToTake(onMic, [MIC, CABLE], false, null)).toBe(CABLE.name);
    expect(cableToTake(onMic, [MIC], false, null)).toBeNull();
  });

  it('also restarts an engine whose device went away', () => {
    expect(cableToTake({ ready: false, error: 'device lost', rocksmith: false }, [CABLE], false, null)).toBe(CABLE.name);
  });

  it('leaves alone an engine still starting, already on the cable, or an input chosen by hand', () => {
    expect(cableToTake({ ready: false, error: null, rocksmith: false }, [CABLE], false, null)).toBeNull();
    expect(cableToTake({ ready: true, error: null, rocksmith: true }, [CABLE], false, null)).toBeNull();
    expect(cableToTake(onMic, [MIC, CABLE], true, null)).toBeNull();
  });

  it('is not tried again until it is unplugged', () => {
    expect(cableToTake(onMic, [CABLE], false, CABLE.name)).toBeNull();
  });
});
