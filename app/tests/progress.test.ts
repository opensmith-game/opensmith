import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { parseChart } from '../src/chart';
import { emptyScore } from '../src/game/types';
import type { PhraseResult } from '../src/game/view';
import { betterGrade, mastery, record, recordPass } from '../src/progress';
import { accelerated, leadIn, riffLoop, roundSpeed } from '../src/riff';

const chart = parseChart(JSON.parse(readFileSync(new URL('../../engine-rt/game/tests/fixtures/demo_rock.json', import.meta.url), 'utf8')));
// Phrases: rest A B A B rest.
const results = (a: [number, number, number], b: [number, number, number]) =>
  new Map<number, PhraseResult>([
    [1, { accuracy: a[0], level: a[1], nextLevel: a[2] }],
    [2, { accuracy: b[0], level: b[1], nextLevel: b[2] }],
  ]);

describe('progress', () => {
  it('a first play sets the levels to start from next time, without records', () => {
    const score = { ...emptyScore(), hits: 20, misses: 2, points: 2500, bestStreak: 15 };
    const out = record(undefined, chart, score, results([0.95, 0, 1], [0.6, 0, 0]), 'A');
    expect(out.records).toEqual([]);
    expect(out.progress.levels).toEqual({ A: 1, B: 0 });
    expect(out.progress.mastered).toEqual({ A: 0 });
    expect(out.progress).toMatchObject({ plays: 1, bestScore: 2500, bestStreak: 15, bestGrade: 'A' });
    expect(out.masteryAfter).toBeGreaterThan(0);
  });

  it('a better play beats records and raises mastery', () => {
    const first = record(undefined, chart, { ...emptyScore(), hits: 10, misses: 5, points: 1000, bestStreak: 6 }, results([0.9, 0, 1], [0.5, 0, 0]), 'C');
    const second = record(first.progress, chart, { ...emptyScore(), hits: 30, points: 4000, bestStreak: 30 }, results([1, 1, 2], [0.9, 0, 1]), 'S');
    expect(second.records).toEqual(['score', 'streak', 'grade', 'mastery']);
    expect(second.masteryAfter).toBeGreaterThan(second.masteryBefore);
    expect(second.progress.levels).toEqual({ A: 2, B: 1 });
    expect(second.progress.mastered).toEqual({ A: 1, B: 0 });
    expect(second.progress.plays).toBe(2);
  });

  it('a song is fully mastered when every phrase is at the top level', () => {
    const top = chart.arrangement.maxDifficulty;
    const p = record(undefined, chart, emptyScore(), results([1, top, top], [0.9, top, top]), 'S').progress;
    expect(mastery(chart, p)).toBeCloseTo(1);
  });

  it('keeps the best grade', () => {
    expect(betterGrade('A', 'B')).toBe('A');
    expect(betterGrade('B', 'S')).toBe('S');
    expect(betterGrade(null, 'D')).toBe('D');
  });
});

describe('Loop Trainer', () => {
  it('a pass at full speed counts for levels and mastery, not for plays or records', () => {
    const song = record(undefined, chart, { ...emptyScore(), hits: 10, misses: 5, points: 1000 }, results([0.6, 0, 0], [0.5, 0, 0]), 'C');
    const pass = recordPass(song.progress, chart, new Map([[2, { accuracy: 0.95, level: 0, nextLevel: 1 }]]));
    expect(pass.progress.levels).toEqual({ A: 0, B: 1 });
    expect(pass.progress.mastered).toEqual({ B: 0 });
    expect(pass.progress.plays).toBe(1);
    expect(pass.progress.bestScore).toBe(1000);
    expect(pass.records).toEqual(['mastery']);
    expect(pass.masteryAfter).toBeGreaterThan(pass.masteryBefore);
  });

  it('the loop starts on a bar before the riff and leaves time to judge the last note', () => {
    const b = chart.phrases[2];
    const loop = riffLoop(chart, 2, 3, 0.03);
    expect(loop.from).toBe(b.start);
    expect(loop.to).toBe(chart.phrases[3].end);
    expect(loop.end).toBeCloseTo(loop.to + 0.63);
    expect(b.start - loop.start).toBeGreaterThanOrEqual(1.5);
    expect(b.start - loop.start).toBeLessThan(5);
    expect(chart.beats.some((x) => x.bar !== undefined && Math.abs(x.time - loop.start) < 1e-9)).toBe(true);
    expect(leadIn({ ...chart, beats: [] }, 10)).toBe(8);
  });

  it('the Accelerator adds 5 % after a good pass, up to full speed', () => {
    expect(accelerated(0.75, 0.92)).toBeCloseTo(0.8);
    expect(accelerated(0.75, 0.8)).toBe(0.75);
    expect(accelerated(0.98, 1)).toBe(1);
    expect(roundSpeed(0.3)).toBeCloseTo(0.4);
    expect(roundSpeed(0.77)).toBeCloseTo(0.75);
  });

  it('a play at a difficulty forced in the pause menu keeps the levels learnt', () => {
    const before = record(undefined, chart, { ...emptyScore(), hits: 10 }, results([0.9, 0, 1], [0.6, 0, 0]), 'B').progress;
    const forced = record(before, chart, { ...emptyScore(), hits: 20 }, results([0.95, 2, 2], [0.9, 2, 2]), 'A', new Date(), false).progress;
    expect(forced.levels).toEqual(before.levels);
    expect(forced.mastered).toEqual({ A: 2, B: 2 });
    expect(forced.plays).toBe(2);
    expect(recordPass(before, chart, results([1, 2, 2], [1, 2, 2]), new Date(), false).progress.levels).toEqual(before.levels);
  });
});
