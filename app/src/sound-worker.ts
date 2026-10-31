/** Makes every sound away from the screen's thread (half a second of work), the interface's first. */
import { rateOf, SOUND_NAMES, synthesize } from './sounds';

for (const name of SOUND_NAMES) {
  const samples = synthesize(name, rateOf(name));
  (self as unknown as Worker).postMessage({ name, rate: rateOf(name), samples }, [samples.buffer]);
}
