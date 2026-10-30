import { localCareer } from '../career/career';
import { parseChart, type Chart } from '../chart';
import { DEFAULT_LOCAL, LocalGame, type LocalGameConfig } from '../game/local';
import type { GameEvent } from '../game/types';
import { localProgress } from '../progress';
import type { Engine, Frame, RiffLoop, SongEntry } from './types';

/**
 * Browser engine: the backing track through Web Audio and an auto-player instead of a guitar.
 * The song clock follows the audio output when it runs; before the first user gesture (or
 * without a backing track) it follows performance.now(). Slowed down, the music also drops in
 * pitch here (the native engine keeps it); the Loop Trainer loop jumps back from the clock.
 */
export class DemoEngine implements Engine {
  readonly kind = 'demo' as const;
  readonly progress = localProgress;
  readonly career = localCareer;
  private levels: Record<string, number> = {};
  config: LocalGameConfig = { ...DEFAULT_LOCAL };
  private ctx: AudioContext | null = null;
  private buffer: AudioBuffer | null = null;
  private source: AudioBufferSourceNode | null = null;
  private gain: GainNode | null = null;
  private volume = 1;
  private chart: Chart | null = null;
  private game: LocalGame | null = null;
  private pending: GameEvent[] = [];
  private running = false;
  private held = 0;
  /** Song time at the last anchor, and when it was heard (performance.now() ms, context s). */
  private anchorSong = 0;
  private anchorPerf = 0;
  private anchorAudio = 0;
  private onAudioClock = false;
  private ended = false;
  private rate = 1;
  private riff: RiffLoop | null = null;

  constructor(private readonly root = '/library/') {}

  get playing(): boolean {
    return this.running;
  }

  /** Backing track gain (1 = as recorded), at once. */
  setVolume(gain: number): void {
    this.volume = gain;
    if (this.gain) this.gain.gain.value = gain;
  }

  unlock(): void {
    this.ctx ??= new AudioContext({ latencyHint: 'interactive' });
    void this.ctx.resume();
  }

  async coverUrl(song: SongEntry): Promise<string | null> {
    return song.cover ? this.root + song.cover.split('/').map(encodeURIComponent).join('/') : null;
  }

  async library(): Promise<SongEntry[]> {
    const r = await fetch(this.root + 'index.json');
    return r.ok ? ((await r.json()) as SongEntry[]) : [];
  }

  async load(id: string, levels: Record<string, number> = {}): Promise<Chart> {
    this.levels = levels;
    const url = this.root + id.split('/').map(encodeURIComponent).join('/');
    const r = await fetch(url);
    if (!r.ok) throw new Error(`${id} : ${r.status}`);
    const chart = parseChart(await r.json());
    this.pause();
    this.buffer = null;
    if (chart.audio.backing) {
      try {
        this.ctx ??= new AudioContext({ latencyHint: 'interactive' });
        const dir = url.slice(0, url.lastIndexOf('/') + 1);
        const data = await (await fetch(dir + encodeURIComponent(chart.audio.backing))).arrayBuffer();
        this.buffer = await this.ctx.decodeAudioData(data);
      } catch (e) {
        console.warn('piste d’accompagnement indisponible, lecture muette', e);
      }
    }
    this.chart = chart;
    this.held = 0;
    this.ended = false;
    this.rate = 1;
    this.riff = null;
    this.game = new LocalGame(chart, this.config, -Infinity, this.levels);
    this.pending.push({ type: 'reset' });
    return chart;
  }

  play(): void {
    if (!this.chart || this.running) return;
    this.running = true;
    this.anchor(this.held, performance.now());
  }

  pause(): void {
    if (!this.running) return;
    this.held = this.time(performance.now());
    this.running = false;
    this.stopSource();
  }

  seek(time: number): void {
    const was = this.running;
    this.pause();
    this.held = time;
    this.ended = false;
    if (this.chart) {
      this.game = this.newGame(time);
      this.pending.push({ type: 'reset' });
    }
    if (was) this.play();
  }

  setSpeed(rate: number): void {
    const now = performance.now();
    const t = this.time(now);
    this.rate = rate;
    if (this.running) this.anchor(t, now);
    else this.held = t;
  }

  setRiff(riff: RiffLoop | null): void {
    this.riff = riff;
    if (riff) this.seek(riff.start);
  }

  setDifficulty(dynamic: boolean, level: number): void {
    this.config = { ...this.config, dynamic, startLevel: level };
    this.game?.setDifficulty(dynamic, level);
  }

  frame(nowMs: number): Frame {
    if (this.running && this.audioRunning() !== this.onAudioClock) this.anchor(this.time(nowMs), nowMs);
    let time = this.time(nowMs);
    const events = this.pending;
    this.pending = [];
    if (this.game) events.push(...this.game.advance(time));
    const riff = this.riff;
    if (riff && this.running && this.game && time >= riff.end) {
      // Time round: the pass is over (the tail let every note be judged), the next one starts.
      events.push({ type: 'pass', score: this.game.currentScore });
      time = riff.start + ((time - riff.end) % (riff.end - riff.start));
      this.game = this.newGame(riff.start);
      events.push({ type: 'reset' });
      this.anchor(time, nowMs);
      events.push(...this.game.advance(time));
    }
    if (!riff && this.chart && !this.ended && time > this.chart.meta.duration + 0.5) {
      this.ended = true;
      events.push({ type: 'end' });
    }
    return { time, playing: this.running, events, peak: 0 };
  }

  /** A game from `start`, keeping the levels learnt so far (as the native engine does). */
  private newGame(start: number): LocalGame {
    const levels = this.game?.learnedLevels() ?? this.levels;
    const range: [number, number] = this.riff ? [this.riff.from, this.riff.to] : [-Infinity, Infinity];
    return new LocalGame(this.chart!, this.config, start, levels, range);
  }

  private audioRunning(): boolean {
    return this.ctx?.state === 'running' && this.buffer !== null;
  }

  /** Audio context time of the sample leaving the speakers at `nowMs`. */
  private heard(ctx: AudioContext, nowMs: number): number {
    const ts = ctx.getOutputTimestamp?.();
    if (ts?.contextTime !== undefined && ts.performanceTime) return ts.contextTime + (nowMs - ts.performanceTime) / 1000;
    return ctx.currentTime - (ctx.outputLatency || ctx.baseLatency || 0);
  }

  private time(nowMs: number): number {
    if (!this.running) return this.held;
    const elapsed =
      this.onAudioClock && this.ctx ? this.heard(this.ctx, nowMs) - this.anchorAudio : (nowMs - this.anchorPerf) / 1000;
    return this.anchorSong + elapsed * this.rate;
  }

  /** Song time `t` is heard at `nowMs`: (re)starts the clock and the backing track from there. */
  private anchor(t: number, nowMs: number): void {
    this.stopSource();
    this.anchorSong = t;
    this.anchorPerf = nowMs;
    this.onAudioClock = this.audioRunning();
    const { ctx, buffer, chart } = this;
    if (!this.onAudioClock || !ctx || !buffer || !chart) return;
    const heard = this.heard(ctx, nowMs);
    this.anchorAudio = heard;
    // During the count-in the track starts when song time 0 is heard.
    const start = Math.max(ctx.currentTime + 0.02, heard - t / this.rate);
    const offset = t + (start - heard) * this.rate + chart.audio.offset;
    if (offset >= buffer.duration) return;
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.playbackRate.value = this.rate;
    if (!this.gain) {
      this.gain = ctx.createGain();
      this.gain.gain.value = this.volume;
      this.gain.connect(ctx.destination);
    }
    src.connect(this.gain);
    src.start(start, Math.max(0, offset));
    this.source = src;
  }

  private stopSource(): void {
    if (!this.source) return;
    try {
      this.source.stop();
    } catch {
      // already stopped
    }
    this.source.disconnect();
    this.source = null;
  }
}
