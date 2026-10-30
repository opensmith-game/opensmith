/**
 * Song clock for the renderer. The engine reports (local time, song time) samples — every frame
 * for the native engine, with IPC jitter — and the model extrapolates between them: small
 * errors are slewed away a fraction at a time, seeks, pauses and rate changes snap.
 */
export class ClockSync {
  private baseLocal = 0;
  private baseSong = 0;
  private rate = 1;
  private playing = false;
  private ready = false;

  /**
   * @param snap error (s) above which the model jumps instead of slewing
   * @param gain fraction of the error corrected per sample
   */
  constructor(
    private readonly snap = 0.05,
    private readonly gain = 0.1,
  ) {}

  /** Song time (s) at local time `localMs` (performance.now() milliseconds). */
  at(localMs: number): number {
    if (!this.playing) return this.baseSong;
    return this.baseSong + ((localMs - this.baseLocal) / 1000) * this.rate;
  }

  sample(localMs: number, song: number, playing: boolean, rate = 1): void {
    if (!this.ready || playing !== this.playing || rate !== this.rate) {
      this.reset(localMs, song, playing, rate);
      return;
    }
    const predicted = this.at(localMs);
    const error = song - predicted;
    if (!playing || Math.abs(error) > this.snap) {
      this.reset(localMs, song, playing, rate);
      return;
    }
    this.baseSong = predicted + error * this.gain;
    this.baseLocal = localMs;
  }

  private reset(localMs: number, song: number, playing: boolean, rate: number): void {
    this.baseLocal = localMs;
    this.baseSong = song;
    this.playing = playing;
    this.rate = rate;
    this.ready = true;
  }
}
