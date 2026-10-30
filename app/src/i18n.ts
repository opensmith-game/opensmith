/**
 * Interface language: English or French.
 *
 * The interface is written in French in the code; English comes from a dictionary keyed by the
 * French text (`locales/en`). `t('Morceaux')` gives "Songs" in English, `t('{n} notes', { n })`
 * fills its placeholders, `tn(n, …)` picks the singular or plural. The static text of
 * index.html is translated in place at startup by `localize`.
 *
 * Changing the language reloads the page: every screen is then built in the new language.
 */
import { EN } from './locales/en';

export type Lang = 'en' | 'fr';
type Vars = Record<string, string | number>;

export const LANGS: { id: Lang; label: string }[] = [
  { id: 'en', label: 'English' },
  { id: 'fr', label: 'Français' },
];
const KEY = 'opensmith.lang';
/** Saved by the French-only versions: whoever has them keeps the French they know. */
const EARLIER = ['opensmith.settings', 'opensmith.progress', 'opensmith.welcomed', 'opensmith.career'];

function initial(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === 'en' || saved === 'fr') return saved;
    if (EARLIER.some((k) => localStorage.getItem(k) !== null)) return 'fr';
  } catch {
    // no storage (tests)
  }
  const system = typeof navigator === 'undefined' ? '' : (navigator.language ?? '');
  return /^fr\b/i.test(system) ? 'fr' : 'en';
}

let current: Lang = initial();

export function lang(): Lang {
  return current;
}

/** Saves the language and rebuilds the interface in it. */
export function setLang(next: Lang): void {
  if (next === current) return;
  try {
    localStorage.setItem(KEY, next);
  } catch {
    // not saved: this session only
  }
  location.reload();
}

/** The language for this page only, nothing saved (tests). */
export function useLang(next: Lang): void {
  current = next;
}

function fill(text: string, vars?: Vars): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (all, name: string) => (name in vars ? String(vars[name]) : all));
}

/** `fr` in the interface language, `{name}` placeholders filled from `vars`. */
export function t(fr: string, vars?: Vars): string {
  return fill(current === 'en' ? (EN[fr] ?? fr) : fr, vars);
}

/**
 * Singular or plural for `n` (French: 0 and 1 are singular; English: only 1), with `{n}` and
 * the other placeholders filled: `tn(count, '{n} morceau', '{n} morceaux')`.
 */
export function tn(n: number, one: string, many: string, vars?: Vars): string {
  const singular = current === 'en' ? n === 1 : Math.abs(n) < 2;
  return t(singular ? one : many, { n, ...vars });
}

const ATTRIBUTES = ['title', 'placeholder', 'aria-label'];

/** Translates, in place, the text and the tooltips of `root` written in French (index.html). */
export function localize(root: ParentNode = document.body): void {
  document.documentElement.lang = current;
  if (current === 'fr') return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const raw = node.nodeValue ?? '';
    const key = raw.replace(/\s+/g, ' ').trim();
    const en = key && EN[key];
    if (en) node.nodeValue = raw.slice(0, raw.length - raw.trimStart().length) + en + raw.slice(raw.trimEnd().length);
  }
  for (const el of root.querySelectorAll<HTMLElement>(ATTRIBUTES.map((a) => `[${a}]`).join(','))) {
    for (const a of ATTRIBUTES) {
      const value = el.getAttribute(a);
      if (value && EN[value]) el.setAttribute(a, EN[value]);
    }
  }
}
