/**
 * The weekly challenge: a song of the library, the same all week, to play as a Score Attack at a
 * difficulty set by the act. Finished without failing, the run reaches bronze, silver or gold by
 * its accuracy; each tier pays its prize once a week, a better one the difference.
 */

import { t } from '../i18n';
import type { AttackDifficulty } from '../score-attack';
import type { Career } from './career';

export interface Challenge {
  /** The week it belongs to. */
  key: string;
  song: string;
  level: AttackDifficulty;
  /** The prize for gold (€). */
  prize: number;
}

export const TIERS: readonly { name: string; accuracy: number; share: number }[] = [
  { name: 'Bronze', accuracy: 0.6, share: 0.4 },
  { name: t('Argent'), accuracy: 0.8, share: 0.7 },
  { name: t('Or'), accuracy: 0.92, share: 1 },
];

const PRIZES: Record<AttackDifficulty, number> = { easy: 40, medium: 80, hard: 150, master: 250 };

/** The two difficulties of an act's challenges, one week after the other. */
const LEVELS: AttackDifficulty[][] = [
  ['easy', 'medium'],
  ['medium', 'hard'],
  ['hard', 'master'],
];

export const weekKey = (c: Pick<Career, 'season' | 'week'>): string => `${c.season}-${c.week}`;

function hash(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return h >>> 0;
}

/** This week's challenge among `songs` (those with audio), or null without any. */
export function weeklyChallenge(c: Pick<Career, 'season' | 'week' | 'act'>, songs: readonly { id: string }[]): Challenge | null {
  if (!songs.length) return null;
  const key = weekKey(c);
  const pool = [...songs].sort((a, b) => a.id.localeCompare(b.id));
  const levels = LEVELS[Math.min(c.act, LEVELS.length) - 1];
  const level = levels[c.week % 2];
  return { key, song: pool[hash(key) % pool.length].id, level, prize: PRIZES[level] };
}

/** The tier a run reached: 0 when it failed or fell short of bronze, then 1 to 3. Judged on
 * the accuracy as the results show it, in whole percents. */
export function tierOf(run: { failed: boolean; accuracy: number }): number {
  if (run.failed) return 0;
  const shown = Math.round(run.accuracy * 100);
  let tier = 0;
  TIERS.forEach((t, i) => {
    if (shown >= Math.round(t.accuracy * 100)) tier = i + 1;
  });
  return tier;
}

/** What tier `tier` pays in all, by 5 €. */
export const tierPrize = (ch: Challenge, tier: number): number => (tier > 0 ? Math.round((ch.prize * TIERS[tier - 1].share) / 5) * 5 : 0);

/** The best tier reached this week, 0 for none. */
export const bestTier = (c: Pick<Career, 'challenge'>, ch: Challenge): number => (c.challenge?.key === ch.key ? c.challenge.tier : 0);

/** A run of the challenge into the career: a better tier is kept and pays the difference. */
export function settleChallenge(c: Career, ch: Challenge, run: { failed: boolean; accuracy: number }): { career: Career; tier: number; paid: number } {
  const before = bestTier(c, ch);
  const tier = tierOf(run);
  if (tier <= before) return { career: c, tier, paid: 0 };
  const paid = tierPrize(ch, tier) - tierPrize(ch, before);
  return { career: { ...c, money: c.money + paid, challenge: { key: ch.key, tier } }, tier, paid };
}
