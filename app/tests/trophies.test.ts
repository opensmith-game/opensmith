import { describe, expect, it } from 'vitest';
import { newCareer, type Career } from '../src/career/career';
import type { PhraseResult } from '../src/game/view';
import { award, careerTrophies, guitarFor, perfectSolo, songTrophies, TROPHIES, type SongFeat } from '../src/trophies';

const feat = (f: Partial<SongFeat> = {}): SongFeat => ({
  notes: 200,
  misses: 3,
  bestStreak: 40,
  grade: 'A',
  mastery: 0.6,
  perfectSolo: false,
  attack: null,
  ...f,
});

// Intro, a solo of two phrases and a rest between them, an outro.
const chart = {
  sections: [
    { name: 'Intro', start: 0, end: 10 },
    { name: 'Guitar Solo', start: 10, end: 30 },
    { name: 'Outro', start: 30, end: 40 },
  ],
  phrases: [
    { name: 'A', start: 0, end: 10 },
    { name: 'B', start: 10, end: 18 },
    { name: 'rest', start: 18, end: 20 },
    { name: 'C', start: 20, end: 30 },
    { name: 'D', start: 30, end: 40 },
  ],
} as unknown as Parameters<typeof perfectSolo>[0];

const results = (accuracies: Record<number, number>): Map<number, PhraseResult> =>
  new Map(Object.entries(accuracies).map(([i, accuracy]) => [Number(i), { accuracy, level: 0, nextLevel: 0 }]));

describe('trophies', () => {
  it('has unique ids, and three of them give a guitar the shop does not sell', () => {
    expect(new Set(TROPHIES.map((t) => t.id)).size).toBe(TROPHIES.length);
    expect(TROPHIES.filter((t) => guitarFor(t.id)).map((t) => [t.id, guitarFor(t.id)!.id])).toEqual([
      ['en-feu', 'lave'],
      ['pro', 'relique'],
      ['solo', 'platine'],
    ]);
  });

  it('rewards a song played without a miss, a long streak, an S, full mastery', () => {
    expect(songTrophies(feat())).toEqual([]);
    expect(songTrophies(feat({ misses: 0 }))).toEqual(['sans-faute']);
    expect(songTrophies(feat({ misses: 0, notes: 120 }))).toEqual([]);
    expect(songTrophies(feat({ bestStreak: 100 }))).toEqual(['centurion']);
    expect(songTrophies(feat({ grade: 'S' }))).toEqual(['legende']);
    expect(songTrophies(feat({ grade: 'S', notes: 30, misses: 0 }))).toEqual([]);
    expect(songTrophies(feat({ mastery: 1 }))).toEqual(['maitrise']);
  });

  it('rewards a hard Score Attack only without a single strike', () => {
    expect(songTrophies(feat({ attack: { level: 'hard', strikes: 0, failed: false } }))).toEqual(['sans-filet']);
    expect(songTrophies(feat({ attack: { level: 'master', strikes: 1, failed: false } }))).toEqual([]);
    expect(songTrophies(feat({ attack: { level: 'medium', strikes: 0, failed: false } }))).toEqual([]);
  });

  it('finds a solo whose phrases were all played without a miss', () => {
    expect(perfectSolo(chart, results({ 0: 0.5, 1: 1, 3: 1, 4: 0.7 }))).toBe(true);
    expect(perfectSolo(chart, results({ 1: 1, 3: 0.95 }))).toBe(false);
    expect(perfectSolo(chart, results({ 1: 1 }))).toBe(false);
  });

  it('rewards gigs and where the career stands', () => {
    const c: Career = newCareer();
    expect(careerTrophies(c, null)).toEqual([]);
    expect(careerTrophies(c, { outcome: 'played', satisfaction: 0.92, encore: true })).toEqual(['premiere', 'rappel', 'en-feu']);
    expect(careerTrophies(c, { outcome: 'booed', satisfaction: 0.1, encore: false })).toEqual(['hue']);
    const far: Career = { ...c, act: 3, stats: { gigs: 30, cleanGigs: 10, encores: 4, earned: 1500 } };
    expect(careerTrophies(far, null)).toEqual(['pro', 'bars', 'resident', 'cachets']);
  });

  it('awards each trophy once, with its guitar', () => {
    const when = new Date('2026-10-28T12:00:00Z');
    const first = award(newCareer(), ['premiere', 'en-feu'], when);
    expect(first.earned.map((t) => t.id)).toEqual(['premiere', 'en-feu']);
    expect(first.career.trophies).toEqual({ premiere: when.toISOString(), 'en-feu': when.toISOString() });
    expect(first.career.guitars).toEqual(['lave']);
    expect(first.career.guitar).toBe('palissandre');
    const again = award(first.career, ['premiere', 'bogus']);
    expect(again.earned).toEqual([]);
    expect(again.career).toBe(first.career);
  });
});
