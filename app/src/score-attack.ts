import { formatTime, startLevelFor } from './chart';
import type { PhraseResult } from './game/view';
import { lang, t } from './i18n';
import type { AttackScore } from './progress';
import './score-attack.css';

/** Score Attack difficulties, easiest first. */
export type AttackDifficulty = 'easy' | 'medium' | 'hard' | 'master';

export interface AttackLevel {
  key: AttackDifficulty;
  label: string;
  /** Starting step of `startLevelFor`: the share of the song's notes played (40 % … all). */
  step: number;
  /** A phrase played below this accuracy costs a strike. */
  pass: number;
  /** The run's points are multiplied by this. */
  bonus: number;
  /** Notes fade out before the strike line on every phrase (played from memory). */
  fade: boolean;
}

export const ATTACK_LEVELS: readonly AttackLevel[] = [
  { key: 'easy', label: t('Facile'), step: 1, pass: 0.4, bonus: 1, fade: false },
  { key: 'medium', label: t('Moyen'), step: 2, pass: 0.5, bonus: 1.5, fade: false },
  { key: 'hard', label: t('Difficile'), step: 4, pass: 0.6, bonus: 2, fade: false },
  { key: 'master', label: t('Maître'), step: 4, pass: 0.7, bonus: 3, fade: true },
];

/** Strikes that end a run. */
export const STRIKES = 3;

export const attackLevel = (key: AttackDifficulty): AttackLevel => ATTACK_LEVELS.find((l) => l.key === key) ?? ATTACK_LEVELS[1];

/** The level the whole song is played at: no dynamic difficulty in Score Attack. */
export const attackStartLevel = (key: AttackDifficulty, maxDifficulty: number, levelNotes?: readonly number[]): number =>
  startLevelFor(attackLevel(key).step, maxDifficulty, levelNotes);

const multiplier = (bonus: number): string => `×${lang() === 'en' ? String(bonus) : String(bonus).replace('.', ',')}`;
/** The interface language's way with numbers and dates. */
const locale = (): string => (lang() === 'en' ? 'en-US' : 'fr-FR');
/** 2nd, 3rd, 4th… */
const ordinal = (n: number): string => {
  const tens = n % 100;
  const suffix = tens >= 11 && tens <= 13 ? 'th' : ['th', 'st', 'nd', 'rd'][n % 10] ?? 'th';
  return `${n}${suffix}`;
};

/**
 * One Score Attack run: the song at a fixed level, each phrase played below
 * the difficulty's bar is a strike, the third one ends the song. The points count with the
 * difficulty's bonus.
 */
export class AttackRun {
  strikes = 0;
  /** Song time of the third strike. */
  failedAt: number | null = null;

  constructor(readonly level: AttackLevel) {}

  get failed(): boolean {
    return this.failedAt !== null;
  }

  /** A phrase was judged at song time `time`: a strike, the run's end, or nothing. */
  phrase(result: PhraseResult, time: number): 'strike' | 'fail' | null {
    if (this.failed || result.accuracy >= this.level.pass) return null;
    this.strikes++;
    if (this.strikes < STRIKES) return 'strike';
    this.failedAt = time;
    return 'fail';
  }

  reset(): void {
    this.strikes = 0;
    this.failedAt = null;
  }

  total(points: number): number {
    return Math.round(points * this.level.bonus);
  }
}

/** The run over the highway: difficulty, bonus, strikes left. */
export class AttackHud {
  private readonly el: HTMLElement;

  /** `hud`: the HUD's root; the badge goes under the phrase map, where the Loop Trainer's does. */
  constructor(hud: HTMLElement) {
    this.el = document.createElement('div');
    this.el.className = 'attack-badge';
    this.el.hidden = true;
    (hud.querySelector('.map') ?? hud).append(this.el);
  }

  show(run: AttackRun | null): void {
    this.el.hidden = !run;
    document.body.classList.toggle('in-attack', !!run);
    if (!run) return;
    const label = document.createElement('b');
    label.textContent = 'SCORE ATTACK';
    const level = document.createElement('span');
    level.textContent = `${run.level.label} · ${multiplier(run.level.bonus)}`;
    const strikes = document.createElement('span');
    strikes.className = 'strikes';
    strikes.title = t('Phrase ratée sous {pct} % : un échec ; {strikes} échecs et le morceau s’arrête', { pct: Math.round(run.level.pass * 100), strikes: STRIKES });
    for (let i = 0; i < STRIKES; i++) {
      const x = document.createElement('i');
      x.textContent = '✕';
      if (i < run.strikes) x.className = 'used';
      strikes.append(x);
    }
    this.el.replaceChildren(label, level, strikes);
    this.el.classList.toggle('danger', run.strikes === STRIKES - 1);
  }
}

/** The difficulties with their best scores, one chosen, and the button to start a run. */
export function attackPicker(songId: string, chosen: AttackDifficulty, best: (key: AttackDifficulty) => AttackScore | undefined): HTMLElement {
  const box = document.createElement('section');
  box.className = 'card-attack';
  const title = document.createElement('h4');
  title.textContent = 'Score Attack';
  const chips = document.createElement('div');
  chips.className = 'attack-levels';
  for (const level of ATTACK_LEVELS) {
    const chip = document.createElement('button');
    chip.className = `attack-level${level.key === chosen ? ' on' : ''}`;
    chip.dataset.lib = 'attack-level';
    chip.dataset.value = level.key;
    const name = document.createElement('b');
    name.textContent = level.label;
    const score = document.createElement('span');
    const top = best(level.key);
    score.textContent = top ? top.score.toLocaleString(locale()) : '—';
    chip.append(name, score);
    chip.title = `${multiplier(level.bonus)} · ${t('une phrase sous {pct} % est un échec', { pct: Math.round(level.pass * 100) })}${level.fade ? ` · ${t('notes qui s’effacent')}` : ''}`;
    chips.append(chip);
  }
  const go = document.createElement('button');
  go.className = 'action';
  go.dataset.lib = 'attack';
  go.dataset.id = songId;
  go.dataset.value = chosen;
  go.textContent = `Score Attack · ${attackLevel(chosen).label}`;
  box.append(title, chips, go);
  return box;
}

/** The end of a run: its score with the bonus, how it ranks, the difficulty's high scores. */
export function attackSummary(run: AttackRun, total: number, rank: number | null, scores: readonly AttackScore[]): HTMLElement {
  const box = document.createElement('section');
  box.className = 'attack-summary';
  const head = document.createElement('div');
  head.className = 'attack-head';
  const label = document.createElement('b');
  label.textContent = `Score Attack · ${run.level.label}`;
  const points = document.createElement('strong');
  points.textContent = total.toLocaleString(locale());
  const bonus = document.createElement('span');
  bonus.textContent = `points (${multiplier(run.level.bonus)})`;
  head.append(label, points, bonus);
  const verdict = document.createElement('p');
  verdict.className = 'attack-verdict';
  if (run.failed) {
    verdict.textContent = t('Échec : {strikes} phrases ratées, arrêt à {time}. Pas de classement.', { strikes: STRIKES, time: formatTime(run.failedAt ?? 0) });
    verdict.classList.add('failed');
  } else if (rank === 1) {
    verdict.textContent = t('Nouveau record !');
    verdict.classList.add('record');
  } else if (rank) {
    verdict.textContent = t('{rank}ᵉ du classement', { rank: lang() === 'en' ? ordinal(rank) : rank });
  } else {
    verdict.textContent = t('Hors du classement');
  }
  box.append(head, verdict);
  if (scores.length) {
    const table = document.createElement('ol');
    table.className = 'attack-scores';
    scores.forEach((s, i) => {
      const row = document.createElement('li');
      if (rank === i + 1) row.className = 'mine';
      const cells = [
        s.score.toLocaleString(locale()),
        t('{n} %', { n: Math.round(s.accuracy * 100) }),
        s.grade,
        new Date(s.date).toLocaleDateString(locale(), { day: 'numeric', month: 'short' }),
      ];
      for (const text of cells) {
        const cell = document.createElement('span');
        cell.textContent = text;
        row.append(cell);
      }
      table.append(row);
    });
    box.append(table);
  }
  return box;
}
