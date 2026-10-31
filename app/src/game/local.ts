import { chordPitches, pitch, type Chart } from '../chart';
import { emptyScore, multiplier, type Expected, type GameEvent, type MissReason, type Score } from './types';

export interface LocalGameConfig {
  hitWindow: number;
  perfectWindow: number;
  lookahead: number;
  dynamic: boolean;
  startLevel: number;
  levelUp: number;
  levelDown: number;
  /** Chance that the auto-player hits each note. */
  skill: number;
  /** Standard deviation of the auto-player's timing error, seconds. */
  jitter: number;
  seed: number;
}

export const DEFAULT_LOCAL: LocalGameConfig = {
  hitWindow: 0.1,
  perfectWindow: 0.04,
  lookahead: 3.2,
  dynamic: true,
  startLevel: 0,
  levelUp: 0.85,
  levelDown: 0.5,
  skill: 0.92,
  jitter: 0.025,
  seed: 7,
};

const CHORD_GRACE = 0.1;
const NOTE_GRACE = 0.06;

interface Planned {
  item: Expected;
  pending: boolean;
  hit: boolean;
  error: number;
}

interface Iteration {
  level: number;
  total: number;
  hits: number;
  activated: boolean;
  done: boolean;
}

/**
 * The rules of engine-rt/game (visibility, judging windows, score, dynamic difficulty) replayed
 * in the browser with an auto-player instead of a guitar: the demo mode.
 */
export class LocalGame {
  private planned: Planned[] = [];
  private iterations: Iteration[];
  private levels = new Map<string, number>();
  private next = 0;
  private cursor = 0;
  private score: Score = emptyScore();
  private rand: () => number;
  /** The auto-player's excuses for its misses (its own draws: the hits do not change). */
  private excuse: () => number;

  /**
   * @param startAt song time where playing starts: earlier notes are not judged.
   * @param range only the notes in [from, to) are shown and judged (Loop Trainer).
   */
  constructor(
    private readonly chart: Chart,
    private cfg: LocalGameConfig = DEFAULT_LOCAL,
    private readonly startAt = -Infinity,
    levels: Record<string, number> = {},
    private readonly range: readonly [number, number] = [-Infinity, Infinity],
  ) {
    const top = chart.arrangement.maxDifficulty;
    for (const [name, level] of Object.entries(levels)) this.levels.set(name, Math.min(level, top));
    this.iterations = chart.phrases.map(() => ({ level: 0, total: 0, hits: 0, activated: false, done: false }));
    this.rand = mulberry32(cfg.seed);
    this.excuse = mulberry32(cfg.seed + 1);
    while (this.next < chart.phrases.length && chart.phrases[this.next].end <= startAt) {
      this.iterations[this.next] = { level: 0, total: 0, hits: 0, activated: true, done: true };
      this.next++;
    }
  }

  /** Levels reached so far by phrase name: where the next game picks up. */
  learnedLevels(): Record<string, number> {
    return Object.fromEntries(this.levels);
  }

  get currentScore(): Score {
    return { ...this.score };
  }

  /** Difficulty changed during the song: the phrases already shown keep their level. */
  setDifficulty(dynamic: boolean, startLevel: number): void {
    this.cfg = { ...this.cfg, dynamic, startLevel };
  }

  level(phrase: string): number {
    const start = Math.min(this.cfg.startLevel, this.chart.arrangement.maxDifficulty);
    return this.cfg.dynamic ? (this.levels.get(phrase) ?? start) : start;
  }

  advance(now: number): GameEvent[] {
    const out: GameEvent[] = [];
    this.activate(now, out);
    let changed = false;
    const w = this.cfg.hitWindow;
    for (let i = this.cursor; i < this.planned.length; i++) {
      const p = this.planned[i];
      if (p.item.time - w > now) break;
      if (!p.pending) continue;
      const grace = p.item.target.kind === 'chord' ? CHORD_GRACE : NOTE_GRACE;
      if (p.hit && now >= p.item.time + p.error) {
        this.hit(p, out);
        changed = true;
      } else if (!p.hit && now > p.item.time + w + grace) {
        this.miss(p, out);
        changed = true;
      }
    }
    while (this.cursor < this.planned.length && !this.planned[this.cursor].pending) this.cursor++;
    this.closePhrases(now, out);
    if (changed) out.push({ type: 'score', score: { ...this.score } });
    return out;
  }

  private activate(now: number, out: GameEvent[]): void {
    const { chart, cfg } = this;
    while (this.next < chart.phrases.length && chart.phrases[this.next].start <= now + cfg.lookahead) {
      const k = this.next++;
      const phrase = chart.phrases[k];
      const level = this.level(phrase.name);
      const from = Math.max(this.startAt, this.range[0]);
      const shown = (levels: [number, number], t: number) =>
        levels[0] <= level && level <= levels[1] && phrase.start <= t && t < phrase.end && t >= from && t < this.range[1];
      const fresh: Expected[] = [];
      chart.notes.forEach((n, event) => {
        if (!shown(n.levels, n.time)) return;
        const target = { kind: 'note' as const, string: n.string, fret: n.fret, midi: pitch(chart, n.string, n.fret) };
        fresh.push({ index: -1, event, time: n.time, sustain: n.sustain, target, iteration: k, level });
      });
      chart.chords.forEach((c, event) => {
        if (!shown(c.levels, c.time)) return;
        const target = { kind: 'chord' as const, template: c.template, midis: chordPitches(chart, c.template) };
        fresh.push({ index: -1, event, time: c.time, sustain: c.sustain, target, iteration: k, level });
      });
      fresh.sort((a, b) => a.time - b.time);
      for (const item of fresh) {
        item.index = this.planned.length;
        const error = Math.max(-0.09, Math.min(0.09, gaussian(this.rand) * cfg.jitter));
        this.planned.push({ item, pending: true, hit: this.rand() < cfg.skill, error });
      }
      this.iterations[k] = { level, total: fresh.length, hits: 0, activated: true, done: false };
      out.push({ type: 'expected', iteration: k, level, items: fresh });
    }
  }

  private hit(p: Planned, out: GameEvent[]): void {
    p.pending = false;
    const perfect = Math.abs(p.error) <= this.cfg.perfectWindow;
    this.iterations[p.item.iteration].hits++;
    const s = this.score;
    s.hits++;
    if (perfect) s.perfects++;
    s.streak++;
    s.bestStreak = Math.max(s.bestStreak, s.streak);
    s.points += (100 + (perfect ? 50 : 0)) * multiplier(s.streak);
    out.push({ type: 'hit', index: p.item.index, time: p.item.time, error: p.error, perfect });
  }

  private miss(p: Planned, out: GameEvent[]): void {
    p.pending = false;
    this.score.misses++;
    this.score.streak = 0;
    out.push({ type: 'miss', index: p.item.index, time: p.item.time, reason: this.reason(p.item) });
  }

  /** Why the auto-player missed: as varied as a real one. */
  private reason(item: Expected): MissReason {
    const r = this.excuse();
    const side = this.excuse() < 0.5 ? -1 : 1;
    if (r < 0.3) return { kind: 'unplayed' };
    if (r < 0.7) return { kind: 'timing', error: side * (0.12 + 0.15 * this.excuse()) };
    if (item.target.kind === 'chord') return { kind: 'wrongChord' };
    return { kind: 'pitch', semitones: side * (1 + Math.floor(2 * this.excuse())) };
  }

  private closePhrases(now: number, out: GameEvent[]): void {
    const w = this.cfg.hitWindow;
    this.chart.phrases.forEach((phrase, k) => {
      const it = this.iterations[k];
      if (!it.activated || it.done || phrase.end + w + CHORD_GRACE >= now) return;
      it.done = true;
      if (it.total === 0) return;
      const accuracy = it.hits / it.total;
      let next = it.level;
      if (this.cfg.dynamic) {
        const top = this.chart.arrangement.maxDifficulty;
        if (accuracy >= this.cfg.levelUp && it.level < top) next = it.level + 1;
        else if (accuracy < this.cfg.levelDown && it.level > 0) next = it.level - 1;
        this.levels.set(phrase.name, next);
      }
      out.push({ type: 'phrase', iteration: k, name: phrase.name, accuracy, level: it.level, nextLevel: next });
    });
  }
}

/** Small deterministic PRNG (uniform in [0, 1)). */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function gaussian(rand: () => number): number {
  const u = Math.max(rand(), 1e-12);
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * rand());
}
