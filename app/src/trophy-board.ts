import type { Career } from './career/career';
import { levelOf } from './career/difficulty';
import { lang, t } from './i18n';
import { guitarFor, TROPHIES, type Trophy } from './trophies';

const CUP =
  '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M8 21h8M12 17v4M7 4h10v5a5 5 0 0 1-10 0V4z"/><path d="M7 6H4a3 3 0 0 0 3 4M17 6h3a3 3 0 0 1-3 4"/></svg>';

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = ''): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

function cup(): HTMLElement {
  const icon = el('span', 'trophy-cup');
  icon.innerHTML = CUP;
  return icon;
}

const locale = (): string => (lang() === 'en' ? 'en-US' : 'fr-FR');
const day = (iso: string): string => new Date(iso).toLocaleDateString(locale(), { day: 'numeric', month: 'long', year: 'numeric' });

/** The trophies screen: every trophy, earned (and when) or still to win (and how). */
export function showTrophies(root: HTMLElement, career: Career): void {
  const won = TROPHIES.filter((t) => t.id in career.trophies).length;
  const panel = el('div', 'panel trophies-panel');
  const head = el('header', 'screen-head');
  const back = el('button', 'action small', t('← Menu'));
  back.type = 'button';
  back.dataset.action = 'menu';
  head.append(back, el('h2', '', t('Trophées')), el('span', 'trophies-count', t('{won} sur {total}', { won, total: TROPHIES.length })));
  panel.append(head, el('p', 'help', t('Sur scène en carrière, et sur les morceaux dans tous les modes sauf le Loop Trainer. Certains donnent une guitare que la boutique ne vend pas ; tous restent, même en recommençant la carrière.')));
  if (career.hallOfFame.length) {
    const hall = el('section', 'hall');
    hall.append(el('h3', '', t('Hall of Fame')));
    for (const h of career.hallOfFame) {
      const row = el('div', 'hall-entry');
      row.append(cup(), el('b', '', h.band), el('span', '', t('{level} · saison {season} · {fans} fans · {date}', { level: levelOf(h.difficulty).label, season: h.season, fans: h.fans.toLocaleString(locale()), date: day(h.date) })));
      hall.append(row);
    }
    panel.append(hall);
  }
  for (const [scope, title] of [
    ['scene', t('Sur scène')],
    ['song', t('Sur les morceaux')],
  ] as const) {
    panel.append(el('h3', '', title));
    const grid = el('div', 'trophy-grid');
    for (const trophy of TROPHIES.filter((x) => x.scope === scope)) {
      const when = career.trophies[trophy.id];
      const card = el('article', `trophy${when ? ' won' : ''}`);
      const text = el('div', 'trophy-text');
      text.append(el('b', '', trophy.name), el('span', '', trophy.blurb));
      const guitar = guitarFor(trophy.id);
      if (guitar) text.append(el('span', 'trophy-reward', t('Guitare {name}', { name: guitar.name })));
      text.append(el('small', '', when ? t('Obtenu le {date}', { date: day(when) }) : t('À gagner')));
      card.append(cup(), text);
      grid.append(card);
    }
    panel.append(grid);
  }
  root.replaceChildren(panel);
}

/** Trophies just earned, announced at the top of the screen for a few seconds each. */
export function announce(box: HTMLElement, trophies: readonly Trophy[]): void {
  trophies.forEach((trophy, i) => {
    const card = el('div', 'trophy-toast');
    card.style.animationDelay = `${i * 0.35}s`;
    const text = el('div', 'trophy-text');
    text.append(el('small', '', t('Trophée débloqué')), el('b', '', trophy.name), el('span', '', trophy.blurb));
    const guitar = guitarFor(trophy.id);
    if (guitar) text.append(el('span', 'trophy-reward', t('Nouvelle guitare : {name}, dans Réglages → Jeu et la boutique', { name: guitar.name })));
    card.append(cup(), text);
    box.append(card);
    window.setTimeout(() => card.remove(), 6500 + i * 350);
  });
}
