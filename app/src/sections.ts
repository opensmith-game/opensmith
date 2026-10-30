import type { Chart } from './chart';
import { t } from './i18n';

/** Names of song parts in the interface language, keyed as tabs (Verse, Pre Chorus, Guitar
 * Solo…) and other charts (verse, prechorus, noguitar…) write them: lowercase, letters only. */
const PARTS: Record<string, string> = {
  intro: 'Intro',
  verse: t('Couplet'),
  couplet: t('Couplet'),
  prechorus: t('Pré-refrain'),
  chorus: t('Refrain'),
  refrain: t('Refrain'),
  postchorus: t('Post-refrain'),
  bridge: t('Pont'),
  pont: t('Pont'),
  solo: 'Solo',
  guitarsolo: 'Solo',
  outro: 'Outro',
  ending: t('Fin'),
  end: t('Fin'),
  coda: 'Coda',
  break: 'Break',
  interlude: 'Interlude',
  riff: 'Riff',
  mainriff: 'Riff',
  breakdown: 'Breakdown',
  transition: 'Transition',
  hook: t('Accroche'),
  tapping: 'Tapping',
  melody: t('Mélodie'),
  buildup: t('Montée'),
  vamp: 'Vamp',
  noguitar: t('Sans guitare'),
  silence: 'Silence',
  ambient: t('Ambiance'),
  fadein: t('Fondu'),
  fadeout: t('Fondu'),
  modverse: t('Couplet modulé'),
  modchorus: t('Refrain modulé'),
  modbridge: t('Pont modulé'),
  head: t('Thème'),
  variation: 'Variation',
};

const EPS = 1e-6;

/** A section's name on screen and its number as written: "Pre Chorus 2" → Pré-refrain, 2.
 * Null for the sections OpenSmith numbers itself ("Section 3"), which say nothing. */
export function partName(raw: string): { name: string; number: string } | null {
  const text = raw.trim();
  if (!text || /^section\s*\d*$/i.test(text)) return null;
  const [, base, number = ''] = /^(.*?)[\s_.#-]*(\d+)?$/.exec(text)!;
  if (!base) return null;
  const key = base
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
    .replace(/[^a-z]/g, '');
  return { name: PARTS[key] ?? base.charAt(0).toUpperCase() + base.slice(1), number };
}

/** On-screen names of the sections: "Couplet 2", "Refrain"; a part that comes back without a
 * number of its own is numbered ("Break 1", "Break 2"). Null for unnamed sections. */
export function sectionLabels(sections: readonly { name: string }[]): (string | null)[] {
  const parts = sections.map((s) => partName(s.name));
  const total = new Map<string, number>();
  for (const p of parts) if (p && !p.number) total.set(p.name, (total.get(p.name) ?? 0) + 1);
  const seen = new Map<string, number>();
  return parts.map((p) => {
    if (!p) return null;
    if (p.number) return `${p.name} ${p.number}`;
    if ((total.get(p.name) ?? 0) < 2) return p.name;
    const k = (seen.get(p.name) ?? 0) + 1;
    seen.set(p.name, k);
    return `${p.name} ${k}`;
  });
}

/** A part of the song and its phrases (indices, inclusive); label '' when it has no name. */
export interface SongPart {
  label: string;
  first: number;
  last: number;
}

type Structure = Pick<Chart, 'phrases' | 'sections'>;

/** The song's parts with their phrases, or null when its sections have no names (charts made
 * from audio alone): phrases then go by their letter. */
export function songParts(chart: Structure): SongPart[] | null {
  const labels = sectionLabels(chart.sections);
  if (labels.every((l) => l === null)) return null;
  const parts: (SongPart & { section: number })[] = [];
  chart.phrases.forEach((p, i) => {
    const section = chart.sections.findIndex((s) => p.start >= s.start - EPS && p.start < s.end - EPS);
    const last = parts[parts.length - 1];
    if (last && last.section === section) last.last = i;
    else parts.push({ label: (section >= 0 && labels[section]) || '', first: i, last: i, section });
  });
  return parts.map(({ label, first, last }) => ({ label, first, last }));
}

const isRest = (name: string): boolean => name.startsWith('rest');

/** What each phrase is called on screen: its part ("Refrain 2", "Couplet 1 · 2/3" when the
 * part has several phrases to play), else "Phrase A"; '' for rests. */
export function phraseLabels(chart: Structure): string[] {
  const parts = songParts(chart);
  return chart.phrases.map((p, i) => {
    if (isRest(p.name)) return '';
    const part = parts?.find((q) => i >= q.first && i <= q.last);
    if (!part?.label) return t('Phrase {name}', { name: p.name });
    const played: number[] = [];
    for (let j = part.first; j <= part.last; j++) if (!isRest(chart.phrases[j].name)) played.push(j);
    return played.length > 1 ? `${part.label} · ${played.indexOf(i) + 1}/${played.length}` : part.label;
  });
}
