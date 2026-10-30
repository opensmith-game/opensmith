import { formatTime, type Chart } from './chart';
import type { RiffLoop } from './engine/types';
import { t, tn } from './i18n';
import { phraseLabels, songParts } from './sections';

/** Slowest speed offered. */
export const MIN_SPEED = 0.4;
/** A pass at least this good lets the Accelerator add a step. */
export const ACCELERATE_AT = 0.9;
const STEP = 0.05;
/** After the last note: time for it to be judged before the loop goes round (s). */
const TAIL = 0.6;

/** What the player picked: phrases `first..last` (indices, inclusive), speed, Accelerator. */
export interface RiffChoice {
  first: number;
  last: number;
  speed: number;
  accelerate: boolean;
}

/** Phrase colour in the picker: how it went so far. */
export type PhraseState = 'mastered' | 'good' | 'poor' | 'played' | '';

export const roundSpeed = (s: number): number => Math.round(Math.min(1, Math.max(MIN_SPEED, s)) / STEP) / (1 / STEP);

/** Speed after a pass with the Accelerator on: one step up after a good one, up to 100 %. */
export const accelerated = (speed: number, accuracy: number): number =>
  accuracy >= ACCELERATE_AT ? roundSpeed(speed + STEP) : speed;

/** "75 %" in French, "75%" in English. */
export const percent = (x: number): string => t('{n} %', { n: Math.round(x * 100) });

/** Where the loop starts: on a bar line at least 1.5 s before `from` (2 s without bars). */
export function leadIn(chart: Chart, from: number): number {
  let start = from - 2;
  for (const b of chart.beats) {
    if (b.time > from - 1.5) break;
    if (b.bar !== undefined) start = b.time;
  }
  return from - start > 5 ? from - 2 : start;
}

/** The loop for phrases `first..last`; `inputOffset` (s) is the calibrated latency. */
export function riffLoop(chart: Chart, first: number, last: number, inputOffset = 0): RiffLoop {
  const from = chart.phrases[first].start;
  const to = chart.phrases[last].end;
  return { start: leadIn(chart, from), end: to + TAIL + Math.max(0, inputOffset), from, to };
}

/** Notes and chords to play from `from` to `to`, all levels. */
export function countNotes(chart: Chart, from: number, to: number): number {
  const inside = (t: number) => t >= from && t < to;
  return chart.notes.filter((n) => inside(n.time)).length + chart.chords.filter((c) => inside(c.time)).length;
}

const TEMPLATE = `
<div class="panel riff-panel">
  <h2>Loop Trainer</h2>
  <p class="help" data-el="help"></p>
  <div class="riff-phrases" data-el="phrases"></div>
  <p class="riff-range" data-el="range"></p>
  <div class="riff-speed">
    <span class="riff-label">${t('Vitesse')}</span>
    <div class="speed-presets" data-el="presets">
      <button class="action small" data-speed="0.5">${percent(0.5)}</button>
      <button class="action small" data-speed="0.75">${percent(0.75)}</button>
      <button class="action small" data-speed="1">${percent(1)}</button>
    </div>
    <input type="range" min="40" max="100" step="5" data-el="speed" />
    <output data-el="speedOut"></output>
  </div>
  <label class="riff-accel"><input type="checkbox" data-el="accelerate" /> ${t('Accélérateur : +5 % de vitesse après chaque passe à 90 % ou plus')}</label>
  <div class="buttons">
    <button class="action primary" data-riff="start">${t('Lancer le riff')}</button>
    <button class="action" data-riff="cancel">${t('Annuler')}</button>
  </div>
</div>`;

export interface RiffHandlers {
  start(choice: RiffChoice): void;
  cancel(): void;
}

/** The Loop Trainer set-up: which phrases, how fast, Accelerator. */
export class RiffPanel {
  private readonly el: Record<string, HTMLElement> = {};
  private chart: Chart | null = null;
  private choice: RiffChoice = { first: 0, last: 0, speed: 0.75, accelerate: true };
  private blocks: HTMLButtonElement[] = [];
  private labels: string[] = [];
  /** Named parts of the song: their button and the phrases to play in them. */
  private parts: { label: string; button: HTMLButtonElement; first: number; last: number }[] = [];

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: RiffHandlers,
  ) {
    root.innerHTML = TEMPLATE;
    root.querySelectorAll<HTMLElement>('[data-el]').forEach((e) => (this.el[e.dataset.el!] = e));
    this.speedInput.addEventListener('input', () => this.setSpeed(Number(this.speedInput.value) / 100));
    this.el.presets.addEventListener('click', (e) => {
      const b = (e.target as HTMLElement).closest<HTMLElement>('[data-speed]');
      if (b) this.setSpeed(Number(b.dataset.speed));
    });
    this.accelerate.addEventListener('change', () => (this.choice.accelerate = this.accelerate.checked));
    root.addEventListener('click', (e) => {
      const action = (e.target as HTMLElement).closest<HTMLElement>('[data-riff]')?.dataset.riff;
      if (action === 'start') this.start();
      else if (action === 'cancel') this.handlers.cancel();
    });
  }

  get visible(): boolean {
    return !this.root.hidden;
  }

  open(chart: Chart, choice: RiffChoice, state: (phrase: number) => PhraseState): void {
    this.chart = chart;
    this.choice = { ...choice, speed: roundSpeed(choice.speed) };
    const total = Math.max(chart.meta.duration, 1);
    const rest = (i: number) => chart.phrases[i].name.startsWith('rest');
    this.labels = phraseLabels(chart);
    this.blocks = chart.phrases.map((p, i) => {
      const b = document.createElement('button');
      b.className = 'riff-block';
      b.type = 'button';
      b.style.flexGrow = String(Math.max(p.end - p.start, 0.01) / total);
      b.dataset.state = state(i);
      b.title = `${this.name(i)} · ${formatTime(p.start)} – ${formatTime(p.end)}`;
      if (rest(i)) b.classList.add('rest');
      const label = document.createElement('span');
      label.textContent = rest(i) ? '' : p.name;
      b.append(label);
      b.addEventListener('click', (e) => this.pick(i, e.shiftKey));
      return b;
    });
    // With named parts (Couplet, Refrain, Solo…), the phrases under the name of their part.
    const parts = songParts(chart);
    this.parts = [];
    this.el.phrases.classList.toggle('parts', !!parts);
    this.el.help.textContent = parts
      ? t('Choisis une partie (clic sur son nom) ou une phrase. Maj + clic pour en prendre plusieurs.')
      : t('Choisis le passage à travailler. Maj + clic pour prendre plusieurs phrases.');
    this.el.phrases.replaceChildren(
      ...(parts ?? []).map((part) => {
        const played = [];
        for (let i = part.first; i <= part.last; i++) if (!rest(i)) played.push(i);
        const group = document.createElement('div');
        group.className = 'riff-part';
        group.style.flexGrow = String(Math.max(chart.phrases[part.last].end - chart.phrases[part.first].start, 0.01) / total);
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'riff-part-name';
        button.textContent = part.label;
        button.title = part.label;
        button.disabled = !part.label || !played.length;
        const row = document.createElement('div');
        row.className = 'riff-part-phrases';
        row.append(...this.blocks.slice(part.first, part.last + 1));
        group.append(button, row);
        if (played.length) {
          const span = { label: part.label, button, first: played[0], last: played[played.length - 1] };
          button.addEventListener('click', (e) => this.pickPart(span.first, span.last, e.shiftKey));
          this.parts.push(span);
        }
        return group;
      }),
    );
    if (!parts) this.el.phrases.replaceChildren(...this.blocks);
    this.accelerate.checked = this.choice.accelerate;
    this.setSpeed(this.choice.speed);
    this.render();
    this.root.hidden = false;
  }

  close(): void {
    this.root.hidden = true;
  }

  /** Launches with the current choice, if the passage has notes. */
  start(): void {
    if (this.notes() > 0) this.handlers.start({ ...this.choice });
  }

  private pick(i: number, extend: boolean): void {
    const c = this.choice;
    const anchor = i < c.first ? c.last : c.first;
    if (extend) this.pickRange(Math.min(anchor, i), Math.max(anchor, i));
    else this.pickRange(i, i);
  }

  /** A part's phrases, or the current choice stretched to them. */
  private pickPart(first: number, last: number, extend: boolean): void {
    const c = this.choice;
    if (extend) this.pickRange(Math.min(c.first, first), Math.max(c.last, last));
    else this.pickRange(first, last);
  }

  private pickRange(first: number, last: number): void {
    this.choice.first = first;
    this.choice.last = last;
    this.render();
  }

  /** A phrase's name on screen: its part and place, or its letter. */
  private name(i: number): string {
    return this.labels[i] || t('Silence');
  }

  private setSpeed(speed: number): void {
    this.choice.speed = roundSpeed(speed);
    this.speedInput.value = String(Math.round(this.choice.speed * 100));
    this.el.speedOut.textContent = percent(this.choice.speed);
    this.el.presets.querySelectorAll<HTMLElement>('[data-speed]').forEach((b) => {
      b.classList.toggle('on', Math.abs(Number(b.dataset.speed) - this.choice.speed) < 1e-6);
    });
  }

  private notes(): number {
    const chart = this.chart;
    if (!chart) return 0;
    return countNotes(chart, chart.phrases[this.choice.first].start, chart.phrases[this.choice.last].end);
  }

  private render(): void {
    const chart = this.chart;
    if (!chart) return;
    const { first, last } = this.choice;
    this.blocks.forEach((b, i) => b.classList.toggle('on', i >= first && i <= last));
    this.parts.forEach((p) => p.button.classList.toggle('on', first <= p.first && last >= p.last));
    const a = chart.phrases[first];
    const z = chart.phrases[last];
    // Whole parts go by their name ("Couplet 1 → Refrain 1"), other choices by their phrases.
    const from = this.parts.find((p) => p.first === first);
    const to = this.parts.find((p) => p.last === last);
    const names =
      from && to
        ? from === to
          ? from.label
          : `${from.label} → ${to.label}`
        : first === last
          ? this.name(first)
          : `${this.name(first)} → ${this.name(last)}`;
    const notes = this.notes();
    this.el.range.textContent = notes
      ? `${names} · ${formatTime(a.start)} – ${formatTime(z.end)} · ${tn(notes, '{n} note', '{n} notes')}`
      : t('{names} : aucune note à jouer ici, choisis une autre phrase.', { names });
    this.el.range.classList.toggle('error', !notes);
    this.root.querySelector<HTMLButtonElement>('[data-riff="start"]')!.disabled = !notes;
  }

  private get speedInput(): HTMLInputElement {
    return this.el.speed as HTMLInputElement;
  }

  private get accelerate(): HTMLInputElement {
    return this.el.accelerate as HTMLInputElement;
  }
}
