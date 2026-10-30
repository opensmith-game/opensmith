import { logoUrl } from '../band';
import { lang, t } from '../i18n';
import type { Souvenir } from './career';

/** The ink of the garage years' flyers, and the colour of each bar's posters. */
const INKS: Record<string, string> = {
  garage: '#1b1b1b',
  anniversaire: '#b0175a',
  quartier: '#1559b0',
  lycee: '#1f6e33',
  saloon: '#ffb547',
  zinc: '#3ad8c8',
  pub: '#6ad86a',
  bistrot: '#ff4a5a',
  soussol: '#7a6aff',
  usine: '#ff8a2a',
  neonclub: '#2af0ff',
  machine: '#ff3ad8',
  showcase: '#ffd166',
};

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = ''): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

const day = (iso: string): string => new Date(iso).toLocaleDateString(lang() === 'en' ? 'en-US' : 'fr-FR', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' });

/** What the night left written across the poster. */
function stamp(s: Souvenir): string | null {
  if (s.outcome === 'booed') return t('Sous les sifflets');
  if (s.outcome === 'abandoned') return t('Écourté');
  if (s.satisfaction >= 0.85) return t('Complet !');
  return null;
}

/** A gig's poster: a photocopied flyer in the garage years, a gig poster in the bars, a club
 * poster with its neon stripe after. */
export function poster(s: Souvenir): HTMLElement {
  const style = s.act >= 3 ? 'poster-bar poster-club' : s.act === 2 ? 'poster-bar' : 'poster-flyer';
  const card = el('article', `poster ${style}${s.decisive ? ' decisive' : ''}`);
  card.style.setProperty('--ink', INKS[s.venue] ?? '#ffd166');
  // Pinned slightly askew, always the same way.
  const tilt = ([...s.date].reduce((h, ch) => (h * 31 + ch.charCodeAt(0)) >>> 0, 7) % 7) - 3;
  card.style.setProperty('--tilt', `${tilt * 0.5}deg`);
  if (s.band) {
    const logo = el('img', 'poster-logo');
    logo.src = logoUrl(s.band);
    logo.alt = s.band.name;
    card.append(logo);
  } else card.append(el('b', 'poster-band', t('Le groupe')));
  card.append(el('small', 'poster-live', s.opening ? t('en première partie de {band}', { band: s.opening }) : t('en concert')), el('h4', '', s.name), el('span', 'poster-date', day(s.date)));
  const songs = el('ol', 'poster-setlist');
  for (const title of s.setlist) songs.append(el('li', '', title));
  card.append(songs);
  if (s.encore) card.append(el('span', 'poster-encore', t('Rappel : {song}', { song: s.encore })));
  const mark = stamp(s);
  if (mark) card.append(el('span', `poster-stamp${s.outcome === 'played' ? '' : ' bad'}`, mark));
  card.append(el('footer', '', t('Saison {season} · semaine {week} · {n} personnes', { season: s.season, week: s.week, n: s.audience })));
  return card;
}
