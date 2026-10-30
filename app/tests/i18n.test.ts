import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { t, tn, useLang } from '../src/i18n';
import { EN } from '../src/locales/en';

const SRC = join(__dirname, '..', 'src');

function sources(dir = SRC, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      if (name !== 'locales') sources(path, out);
    } else if (name.endsWith('.ts') && name !== 'i18n.ts') out.push(path);
  }
  return out;
}

const STRING = String.raw`'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)"|` + '`((?:\\\\.|[^`\\\\$])*)`';
const unescape = (s: string) => s.replace(/\\n/g, '\n').replace(/\\(.)/g, '$1');

/** The French texts given to t() and tn() in the code. */
function keys(code: string): string[] {
  const found: string[] = [];
  for (const m of code.matchAll(new RegExp(String.raw`(?<![\w.])t\(\s*(?:${STRING})`, 'g'))) found.push(unescape(m[1] ?? m[2] ?? m[3]));
  const plural = new RegExp(String.raw`(?<![\w.])tn\([^,]+,\s*(?:${STRING})\s*,\s*(?:${STRING})`, 'g');
  for (const m of code.matchAll(plural)) found.push(unescape(m[1] ?? m[2] ?? m[3]), unescape(m[4] ?? m[5] ?? m[6]));
  return found;
}

/** The text of index.html (outside scripts and styles), as `localize` looks it up. */
function pageTexts(): string[] {
  const html = readFileSync(join(__dirname, '..', 'index.html'), 'utf8')
    .replace(/<(script|style)[\s\S]*?<\/\1>/g, '')
    .replace(/<!--[\s\S]*?-->/g, '');
  const decode = (s: string) => s.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&nbsp;/g, ' ');
  const texts = [...html.matchAll(/>([^<]+)</g)].map((m) => decode(m[1]).replace(/\s+/g, ' ').trim());
  const attributes = [...html.matchAll(/\s(?:title|placeholder|aria-label)="([^"]+)"/g)].map((m) => decode(m[1]));
  return [...texts, ...attributes].filter((s) => /\p{L}/u.test(s));
}

const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

describe('interface language', () => {
  afterEach(() => useLang('fr'));

  it('looks the English up by the French text and fills the placeholders', () => {
    useLang('en');
    const [fr, en] = Object.entries(EN).find(([k]) => placeholders(k).length === 0)!;
    expect(t(fr)).toBe(en);
    expect(t('Pas encore traduit, {who}', { who: 'Léa' })).toBe('Pas encore traduit, Léa');
    useLang('fr');
    expect(t(fr)).toBe(fr);
  });

  it('picks the singular and plural by each language’s rule', () => {
    useLang('fr');
    expect([0, 1, 2].map((n) => tn(n, '{n} pomme', '{n} pommes'))).toEqual(['0 pomme', '1 pomme', '2 pommes']);
    useLang('en');
    expect([0, 1, 2].map((n) => tn(n, '{n} pomme', '{n} pommes'))).toEqual(['0 pommes', '1 pomme', '2 pommes']);
  });

  it('has English for every text the code shows', () => {
    const missing = sources().flatMap((file) => keys(readFileSync(file, 'utf8')).filter((k) => !(k in EN)).map((k) => `${file.slice(SRC.length + 1)}: ${k}`));
    expect(missing).toEqual([]);
  });

  it('has English for every text of the page', () => {
    expect(pageTexts().filter((s) => !(s in EN))).toEqual([]);
  });

  it('keeps the placeholders of the French in the English', () => {
    const broken = Object.entries(EN).filter(([fr, en]) => placeholders(fr).join() !== placeholders(en).join());
    expect(broken).toEqual([]);
  });
});
