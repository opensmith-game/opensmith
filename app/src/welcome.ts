import type { AudioStatus } from './engine/tauri';
import { t } from './i18n';

/** A part of the guitar settings the guide shows on its own. */
export type WelcomePart = 'level' | 'latency';

export interface WelcomeHooks {
  /** The audio devices as the engine sees them (null while it starts). */
  status(): AudioStatus | null;
  /** Shows `part` of the guitar settings in `into`; null: puts them back. */
  settings(part: WelcomePart | null, into?: HTMLElement): void;
  /** Opens the tuner, standard tuning. */
  tune(): void;
  /** Guide finished or skipped. */
  done(): void;
}

const STEPS = [
  {
    key: 'cable',
    label: t('Câble'),
    title: t('Branche ta guitare'),
    text: t('Le câble guitare USB (Real Tone Cable) se branche sur un port USB du Mac, ta guitare au bout. Le casque ou les enceintes restent sur la sortie du Mac.'),
  },
  {
    key: 'level',
    label: t('Niveau'),
    title: t('Règle le niveau d’entrée'),
    text: t('Clique « Régler automatiquement », ne joue pas pendant 2 s, puis gratte fort pendant 4 s.'),
  },
  {
    key: 'latency',
    label: t('Latence'),
    title: t('Mesure la latence'),
    text: t('Clique « Calibrer », écoute les 4 clics d’annonce, puis gratte une corde étouffée pile sur chacun des 12 clics suivants.'),
  },
  {
    key: 'tune',
    label: t('Accordage'),
    title: t('Accorde ta guitare'),
    text: t('Joue chaque corde à vide jusqu’à ce qu’elle soit validée, à 3 cents près. Chaque morceau commence aussi par l’accordeur, réglé sur son accordage.'),
  },
] as const;

const KEY = 'opensmith.welcomed';

/** Whether the guide was already gone through (or skipped) on this Mac. */
export function welcomed(): boolean {
  try {
    return localStorage.getItem(KEY) === '1';
  } catch {
    return true;
  }
}

function remember(): void {
  try {
    localStorage.setItem(KEY, '1');
  } catch {
    // storage unavailable: the guide comes back next time
  }
}

/** What the guide says about the cable, from the engine's status. */
export function cableLine(s: AudioStatus | null): { ok: boolean | null; text: string } {
  if (!s || (!s.ready && !s.error)) return { ok: null, text: t('Le moteur audio démarre…') };
  if (s.error) return { ok: false, text: t('Audio indisponible : {error}', { error: s.error }) };
  if (s.rocksmith) return { ok: true, text: t('Câble guitare USB branché : {input} → {output}.', { input: s.input ?? '', output: s.output ?? '' }) };
  return {
    ok: false,
    text: t('Pas de câble guitare USB : l’app écoute « {input} », sans retour de la guitare. Branche le câble puis relance l’app ; tu peux aussi continuer sans.', {
      input: s.input ?? '',
    }),
  };
}

const TEMPLATE = `
<div class="panel settings-panel welcome-panel">
  <header>
    <small data-el="count"></small>
    <h2 data-el="title"></h2>
    <p data-el="text"></p>
  </header>
  <ol class="welcome-steps" data-el="steps"></ol>
  <div class="welcome-body" data-el="body"></div>
  <div class="buttons">
    <button class="action" data-welcome="skip">${t('Passer le guide')}</button>
    <button class="action" data-welcome="back" data-el="back">${t('Retour')}</button>
    <button class="action primary" data-welcome="next" data-el="next">${t('Suivant')}</button>
  </div>
</div>`;

/** The first-launch guide: cable, input level, latency, tuning; once, then from the settings. */
export class Welcome {
  private step = 0;
  private readonly el: Record<string, HTMLElement> = {};

  constructor(
    private readonly root: HTMLElement,
    private readonly hooks: WelcomeHooks,
  ) {
    root.innerHTML = TEMPLATE;
    root.querySelectorAll<HTMLElement>('[data-el]').forEach((e) => (this.el[e.dataset.el!] = e));
    this.el.steps.replaceChildren(
      ...STEPS.map((s) => {
        const li = document.createElement('li');
        li.textContent = s.label;
        return li;
      }),
    );
    root.addEventListener('click', (e) => {
      const action = (e.target as HTMLElement).closest<HTMLElement>('[data-welcome]')?.dataset.welcome;
      if (action === 'next') this.go(this.step + 1);
      else if (action === 'back') this.go(this.step - 1);
      else if (action === 'skip') this.finish();
      else if (action === 'tune') {
        remember();
        this.close();
        this.hooks.tune();
      }
    });
  }

  get visible(): boolean {
    return !this.root.hidden;
  }

  open(): void {
    this.root.hidden = false;
    this.go(0);
  }

  /** Skipped (Escape or "Passer le guide"): not shown again. */
  finish(): void {
    remember();
    this.close();
    this.hooks.done();
  }

  /** The engine's status changed: the cable step says so. */
  refresh(): void {
    if (this.visible && STEPS[this.step].key === 'cable') this.render();
  }

  private close(): void {
    this.hooks.settings(null);
    this.root.hidden = true;
  }

  private go(step: number): void {
    if (step >= STEPS.length) {
      this.finish();
      return;
    }
    this.step = Math.max(0, step);
    this.render();
  }

  private render(): void {
    const s = STEPS[this.step];
    this.el.count.textContent = t('Premier lancement · étape {n} sur {total}', { n: this.step + 1, total: STEPS.length });
    this.el.title.textContent = s.title;
    this.el.text.textContent = s.text;
    this.el.steps.querySelectorAll('li').forEach((li, k) => {
      li.classList.toggle('on', k === this.step);
      li.classList.toggle('done', k < this.step);
    });
    (this.el.back as HTMLButtonElement).disabled = this.step === 0;
    this.el.next.textContent = this.step === STEPS.length - 1 ? t('Plus tard') : t('Suivant');
    this.el.next.classList.toggle('primary', s.key !== 'tune');
    const body = this.el.body;
    if (s.key === 'level' || s.key === 'latency') {
      this.hooks.settings(s.key, body);
      return;
    }
    this.hooks.settings(null);
    if (s.key === 'cable') {
      const line = cableLine(this.hooks.status());
      const box = document.createElement('p');
      box.className = `cable${line.ok === true ? ' ok' : line.ok === false ? ' warn' : ''}`;
      box.textContent = line.text;
      body.replaceChildren(box);
    } else {
      const tune = document.createElement('button');
      tune.className = 'action primary';
      tune.dataset.welcome = 'tune';
      tune.textContent = t('Accorder maintenant');
      body.replaceChildren(tune);
    }
  }
}
