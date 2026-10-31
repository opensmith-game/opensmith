import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { levelNotes, parseChart, startLevelFor } from '../src/chart';
import { DEFAULT_LOCAL, LocalGame, type LocalGameConfig } from '../src/game/local';
import { accuracy, type GameEvent, type Score } from '../src/game/types';
import { GameView, missText } from '../src/game/view';

/** Same chart as the Rust closed-loop test (engine-rt/game/tests/game_loop.rs): phrases rest A B A B rest. */
const chart = parseChart(
  JSON.parse(readFileSync(new URL('../../engine-rt/game/tests/fixtures/demo_rock.json', import.meta.url), 'utf8')),
);

function play(cfg: Partial<LocalGameConfig>, startAt = -Infinity): GameEvent[] {
  const game = new LocalGame(chart, { ...DEFAULT_LOCAL, ...cfg }, startAt);
  const events: GameEvent[] = [];
  for (let t = -3; t < chart.meta.duration + 1; t += 1 / 120) events.push(...game.advance(t));
  return events;
}

const of = <K extends GameEvent['type']>(events: GameEvent[], type: K) =>
  events.filter((e): e is Extract<GameEvent, { type: K }> => e.type === type);

const finalScore = (events: GameEvent[]): Score => of(events, 'score').at(-1)!.score;

describe('demo game', () => {
  it('a flawless player hits everything and climbs a level per repeated phrase', () => {
    const events = play({ skill: 1, jitter: 0.01 });
    const items = of(events, 'expected').flatMap((e) => e.items);
    expect(items.length).toBeGreaterThan(20);
    expect(items.map((i) => i.index)).toEqual(items.map((_, i) => i));
    expect(items.every((x, i) => i === 0 || x.time >= items[i - 1].time)).toBe(true);
    expect(of(events, 'hit')).toHaveLength(items.length);
    expect(of(events, 'miss')).toHaveLength(0);
    const phrases = of(events, 'phrase').map((p) => [p.name, p.level, p.nextLevel]);
    expect(phrases).toEqual([
      ['A', 0, 1],
      ['B', 0, 1],
      ['A', 1, 2],
      ['B', 1, 2],
    ]);
    const score = finalScore(events);
    expect(accuracy(score)).toBe(1);
    expect(score.bestStreak).toBe(items.length);
  });

  it('a silent player misses everything and stays at the first level', () => {
    const events = play({ skill: 0 });
    expect(of(events, 'hit')).toHaveLength(0);
    expect(of(events, 'phrase').every((p) => p.level === 0 && p.nextLevel === 0)).toBe(true);
    expect(finalScore(events).streak).toBe(0);
  });

  it('starting in the middle of the song skips the earlier phrases', () => {
    const third = chart.phrases[3];
    const events = play({ skill: 1 }, third.start);
    const items = of(events, 'expected').flatMap((e) => e.items);
    expect(items.every((i) => i.time >= third.start)).toBe(true);
    expect(of(events, 'phrase')[0]).toMatchObject({ iteration: 3 });
  });

  it('the view tracks each note, the streak milestones and the phrase results', () => {
    const view = new GameView(chart);
    const events = play({ skill: 1, jitter: 0.01 });
    view.apply(events, 40);
    expect(view.items.every((i) => i.status === 'hit')).toBe(true);
    expect(view.drainEffects()).toHaveLength(view.items.length);
    const toasts = view.drainToasts();
    expect(toasts.some((t) => t.kind === 'streak' && t.title.includes('25'))).toBe(true);
    expect(toasts.filter((t) => t.kind === 'up')).toHaveLength(4);
    view.apply([{ type: 'reset' }], 41);
    expect(view.items).toHaveLength(0);
    expect(view.score.points).toBe(0);
  });

  it('finds the first note still on screen, long sustains included', () => {
    const view = new GameView(chart);
    view.apply(play({ skill: 1 }), 0);
    const t = 12;
    const first = view.firstVisible(t);
    expect(view.items.slice(0, first).every((i) => i.time + i.sustain < t)).toBe(true);
  });
});

describe('why a note was missed', () => {
  it('every miss says why, and a late note explains its miss afterwards', () => {
    const events = play({ skill: 0.5 });
    const misses = of(events, 'miss');
    expect(misses.length).toBeGreaterThan(5);
    expect(new Set(misses.map((m) => m.reason.kind)).size).toBeGreaterThan(2);

    const view = new GameView(chart);
    view.apply(events, 0);
    const missed = view.items.filter((i) => i.status === 'missed');
    expect(missed.every((i) => i.reason)).toBe(true);
    const first = missed[0];
    view.drainEffects();
    view.apply([{ type: 'late', index: first.index, error: 0.2 }], 0);
    expect(first.reason).toEqual({ kind: 'timing', error: 0.2 });
    expect(view.drainEffects().map((e) => e.kind)).toEqual(['late']);
  });

  it('says it in words, except when nothing was played', () => {
    expect(missText({ kind: 'timing', error: -0.2 })).toBe('Trop tôt');
    expect(missText({ kind: 'timing', error: 0.2 })).toBe('Trop tard');
    expect(missText({ kind: 'pitch', semitones: 2 })).toBe('Trop haut');
    expect(missText({ kind: 'pitch', semitones: -1 })).toBe('Trop bas');
    expect(missText({ kind: 'wrongChord' })).toBe('Mauvais accord');
    expect(missText({ kind: 'unplayed' })).toBeNull();
    expect(missText(undefined)).toBeNull();
  });
});

describe('Loop Trainer in the demo game', () => {
  const [, , b] = chart.phrases;

  it('only the phrases of the loop are shown and judged, and levels carry over', () => {
    const range: [number, number] = [b.start, b.end];
    const pass = (levels: Record<string, number>) => {
      const game = new LocalGame(chart, { ...DEFAULT_LOCAL, skill: 1, jitter: 0.01 }, b.start - 2, levels, range);
      const events: GameEvent[] = [];
      for (let t = b.start - 2; t < b.end + 0.5; t += 1 / 120) events.push(...game.advance(t));
      return { game, events };
    };
    const first = pass({});
    const items = of(first.events, 'expected').flatMap((e) => e.items);
    expect(items.length).toBeGreaterThan(5);
    expect(items.every((i) => i.time >= b.start && i.time < b.end)).toBe(true);
    expect(of(first.events, 'phrase').map((p) => [p.name, p.level, p.nextLevel])).toEqual([['B', 0, 1]]);
    expect(first.game.currentScore.hits).toBe(items.length);
    expect(first.game.learnedLevels()).toEqual({ B: 1 });

    const second = pass(first.game.learnedLevels());
    expect(of(second.events, 'phrase').map((p) => [p.name, p.level])).toEqual([['B', 1]]);
  });

  it('the view keeps each pass with its phrase results before the next reset', () => {
    const view = new GameView(chart);
    const score = { hits: 9, perfects: 4, misses: 1, extras: 0, streak: 5, bestStreak: 6, points: 1500 };
    view.apply(
      [
        { type: 'phrase', iteration: 2, name: 'B', accuracy: 0.9, level: 0, nextLevel: 1 },
        { type: 'pass', score },
        { type: 'reset' },
      ],
      0,
    );
    const passes = view.drainPasses();
    expect(passes).toHaveLength(1);
    expect(passes[0].score).toEqual(score);
    expect(passes[0].results.get(2)).toEqual({ accuracy: 0.9, level: 0, nextLevel: 1 });
    expect(view.results.size).toBe(0);
    expect(view.drainPasses()).toHaveLength(0);
  });
});

describe('starting level', () => {
  it('plays the same share of the notes, whatever the number of levels', () => {
    // Notes at each level of a detailed hand-made chart: its lowest levels play a note now and then.
    const detailed = [20, 40, 70, 110, 150, 210, 270, 340, 430, 460, 490, 520, 560, 610, 700, 760, 840, 900];
    expect([0, 1, 2, 3, 4].map((step) => startLevelFor(step, 17, detailed))).toEqual([6, 8, 12, 15, 17]);
    // A chart made from audio: its beginner level is already a third of the song.
    const demo = levelNotes(chart);
    expect(demo[0] / demo[demo.length - 1]).toBeGreaterThan(0.3);
    expect([0, 1, 2, 3, 4].map((step) => startLevelFor(step, chart.arrangement.maxDifficulty, demo))).toEqual([0, 1, 2, 3, 4]);
  });

  it('goes the same share of the way up without note counts', () => {
    expect([0, 1, 2, 3, 4].map((step) => startLevelFor(step, 4))).toEqual([0, 1, 2, 3, 4]);
    expect([0, 1, 2, 3, 4].map((step) => startLevelFor(step, 17))).toEqual([0, 4, 9, 13, 17]);
    expect(startLevelFor(9, 17)).toBe(17);
  });

  it('changes the difficulty in mid-song from the phrases not shown yet', () => {
    const top = chart.arrangement.maxDifficulty;
    const game = new LocalGame(chart, { ...DEFAULT_LOCAL, dynamic: true, startLevel: 0 });
    const events: GameEvent[] = [];
    const change = chart.phrases[1].start;
    let t = -3;
    for (; t < change; t += 1 / 120) events.push(...game.advance(t));
    game.setDifficulty(false, top);
    for (; t < chart.meta.duration + 1; t += 1 / 120) events.push(...game.advance(t));
    const shown = of(events, 'expected');
    expect(shown[0].level).toBe(0);
    expect(shown.at(-1)!.level).toBe(top);
    for (const e of shown) if (chart.phrases[e.iteration].start > change + DEFAULT_LOCAL.lookahead) expect(e.level).toBe(top);
  });
});
