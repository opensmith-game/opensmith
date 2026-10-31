/**
 * When the game makes its sounds, and how loud: the interface (hover, click, back, a setting
 * changed, an error) outside of play, the cues in play (count-in on the song's beat, streaks,
 * a phrase levelled up, the grade, a fanfare), and the crowd (its murmur following the room,
 * cheers, applause, clapping for an encore). The engine plays them, on the same output as the
 * guitar and the song.
 */
import type { Chart } from './chart';
import type { SoundName } from './sounds';

/** What plays the sounds: the engine. */
export interface SoundOut {
  sound(name: SoundName, gain: number, at?: number): void;
  bed(name: SoundName | null, gain: number): void;
}

/** Interface sounds closer together than this are one (s): a mouse sweeping over a menu ticks once. */
const HOVER_GAP = 0.04;
/** Smallest change of the room's level sent to the engine. */
const ROOM_STEP = 0.02;

/** Song times of a count-in into the music: the song's beats before its first one, from `from` on. */
export function countIn(chart: Chart, from: number): number[] {
  const bpm = chart.tempo?.bpm > 0 ? chart.tempo.bpm : 120;
  const beat = 60 / bpm;
  const first = chart.beats[0]?.time ?? 0;
  const times: number[] = [];
  for (let k = 4; k >= 1; k--) {
    const at = first - k * beat;
    // Not in the first moments of the lead-in: the screen has just changed.
    if (at >= from + 0.4) times.push(at);
  }
  return times;
}

export class SoundBoard {
  /** Gains from the settings (already squared: perceptual sliders). */
  private effects = 0.36;
  private crowd = 0.5;
  private lastUi = -Infinity;
  /** The room's background: which, and how full (0..1). */
  private bedName: 'crowd' | 'clapping' | null = null;
  private bedLevel = 0;

  constructor(
    private readonly out: SoundOut,
    private readonly now: () => number = () => performance.now() / 1000,
  ) {}

  /** Slider positions 0..1 (the master scales both). */
  setVolumes(effects: number, crowd: number, master = 1): void {
    this.effects = (effects * master) ** 2;
    this.crowd = (crowd * master) ** 2;
    if (this.bedName) this.sendRoom();
  }

  /** An interface sound. */
  ui(name: 'hover' | 'click' | 'back' | 'tick' | 'error'): void {
    const t = this.now();
    if ((name === 'hover' || name === 'tick') && t - this.lastUi < HOVER_GAP) return;
    this.lastUi = t;
    if (this.effects > 0) this.out.sound(name, this.effects);
  }

  /** A cue in play, now or at song time `at`. */
  cue(name: 'count' | 'countAccent' | 'streak' | 'levelUp' | 'grade' | 'fanfare', at?: number): void {
    if (this.effects > 0) this.out.sound(name, this.effects, at);
  }

  /** Clicks on the beat into the song, the first higher. */
  countIn(chart: Chart, from: number): void {
    countIn(chart, from).forEach((at, k) => this.cue(k === 0 ? 'countAccent' : 'count', at));
  }

  /** The room's murmur at `level` (0 empty and cold, 1 packed and hot), or clapping in time;
   * null: silent. Called every frame on stage: only a change the ear can tell is sent. */
  room(level: number | null, name: 'crowd' | 'clapping' = 'crowd'): void {
    if (level === null) {
      if (this.bedName) this.out.bed(null, 0);
      this.bedName = null;
      return;
    }
    const clamped = Math.min(1, Math.max(0, level));
    if (name === this.bedName && Math.abs(clamped - this.bedLevel) < ROOM_STEP) return;
    this.bedName = name;
    this.bedLevel = clamped;
    this.sendRoom();
  }

  private sendRoom(): void {
    this.out.bed(this.bedName, this.crowd * (0.15 + 0.85 * this.bedLevel));
  }

  /** The crowd cheers, `strength` 0..1. */
  cheer(strength: number): void {
    if (this.crowd > 0 && strength > 0) this.out.sound('cheer', this.crowd * Math.min(1, strength));
  }

  /** Applause, `strength` 0..1. */
  applause(strength: number): void {
    if (this.crowd > 0 && strength > 0) this.out.sound('applause', this.crowd * Math.min(1, strength));
  }
}

/** What counts as something to point at or press. */
const POINTABLE = 'button:not(:disabled), a[href], select, input[type="range"], input[type="checkbox"], [data-action], [data-song], [role="button"]';
/** Presses that go back or leave. */
const BACK = new Set(['menu', 'pause-menu', 'library', 'riff-exit', 'close-amp', 'close-help', 'gig-quit']);

/** Interface sounds on `root`, while `active()` (not during play). */
export function interfaceSounds(root: Document, board: SoundBoard, active: () => boolean): void {
  let over: Element | null = null;
  root.addEventListener('pointerover', (e) => {
    const el = (e.target as Element).closest?.(POINTABLE) ?? null;
    if (el === over) return;
    over = el;
    if (el && active()) board.ui('hover');
  });
  root.addEventListener('click', (e) => {
    const el = (e.target as Element).closest?.(POINTABLE);
    if (!el || !active() || el instanceof HTMLSelectElement || (el instanceof HTMLInputElement && el.type === 'range')) return;
    const action = (el as HTMLElement).dataset?.action ?? '';
    board.ui(BACK.has(action) || el.textContent?.trim().startsWith('←') ? 'back' : 'click');
  });
  root.addEventListener('input', (e) => {
    const el = e.target;
    if (active() && (el instanceof HTMLSelectElement || el instanceof HTMLInputElement)) board.ui('tick');
  });
}
