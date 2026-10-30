import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { parseChart } from '../src/chart';
import type { PhraseResult } from '../src/game/view';
import { emptyProgress, recordAttack } from '../src/progress';
import { attackLevel, attackStartLevel, AttackRun, STRIKES } from '../src/score-attack';

const chart = parseChart(JSON.parse(readFileSync(new URL('../../engine-rt/game/tests/fixtures/demo_rock.json', import.meta.url), 'utf8')));
const result = (accuracy: number, level = 2): PhraseResult => ({ accuracy, level, nextLevel: level });
const run = (score: number) => ({ score, accuracy: 0.9, grade: 'A', streak: 30 });

describe('Score Attack', () => {
  it('three phrases under the bar end the run; good ones cost nothing', () => {
    const r = new AttackRun(attackLevel('hard'));
    expect(r.phrase(result(0.95), 5)).toBeNull();
    expect(r.phrase(result(0.59), 10)).toBe('strike');
    expect(r.phrase(result(0.6), 15)).toBeNull();
    expect(r.phrase(result(0.2), 20)).toBe('strike');
    expect(r.failed).toBe(false);
    expect(r.phrase(result(0.5), 25)).toBe('fail');
    expect(r.strikes).toBe(STRIKES);
    expect(r.failedAt).toBe(25);
    expect(r.phrase(result(0), 30)).toBeNull();
    r.reset();
    expect([r.strikes, r.failed]).toEqual([0, false]);
  });

  it('harder levels play more of the song for a bigger bonus', () => {
    const notes = [10, 20, 40, 60, 80, 100];
    const levels = (['easy', 'medium', 'hard', 'master'] as const).map((k) => attackStartLevel(k, 5, notes));
    expect(levels).toEqual([2, 3, 5, 5]);
    expect(new AttackRun(attackLevel('easy')).total(1000)).toBe(1000);
    expect(new AttackRun(attackLevel('medium')).total(1001)).toBe(1502);
    expect(new AttackRun(attackLevel('master')).total(1000)).toBe(3000);
    expect(attackLevel('master').fade).toBe(true);
  });

  it('runs are ranked per difficulty, the levels of the learning mode stay', () => {
    const before = { ...emptyProgress(), levels: { A: 3, B: 1 } };
    const phrases = new Map([[1, result(0.9, 4)]]);
    const first = recordAttack(before, chart, 'hard', run(5000), phrases, false, new Date('2026-10-01'));
    expect(first.rank).toBe(1);
    expect(first.progress.levels).toEqual({ A: 3, B: 1 });
    expect(first.progress.mastered).toEqual({ A: 4 });
    expect(first.progress.plays).toBe(1);
    const second = recordAttack(first.progress, chart, 'hard', run(3000), new Map(), false, new Date('2026-10-02'));
    expect(second.rank).toBe(2);
    const third = recordAttack(second.progress, chart, 'hard', run(7000), new Map(), false, new Date('2026-10-03'));
    expect(third.rank).toBe(1);
    expect(third.scores.map((s) => s.score)).toEqual([7000, 5000, 3000]);
    expect(third.progress.attack?.easy).toBeUndefined();

    const failed = recordAttack(third.progress, chart, 'hard', run(9000), new Map(), true);
    expect(failed.rank).toBeNull();
    expect(failed.scores).toHaveLength(3);
    expect(failed.progress.plays).toBe(4);

    let p = failed.progress;
    for (const s of [100, 200, 300]) p = recordAttack(p, chart, 'hard', run(s), new Map(), false).progress;
    expect(p.attack!.hard!.map((s) => s.score)).toEqual([7000, 5000, 3000, 300, 200]);
    expect(recordAttack(p, chart, 'hard', run(150), new Map(), false).rank).toBeNull();
  });
});
