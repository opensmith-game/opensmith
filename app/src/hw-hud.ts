import type { AudioStatus, InputDevice } from './engine/tauri';
import { toDb } from './guitar-settings';
import { t } from './i18n';
import type { QueueSummary } from './imports';

/** Segments of the level meter, from −60 to 0 dBFS. */
const SEGMENTS = 10;

/** Whether the engine should move to the USB guitar cable now: it is plugged in, the engine is
 * not on it (or stopped), no other input was chosen by hand, and it was not tried already. */
export function cableToTake(
  s: Pick<AudioStatus, 'ready' | 'error' | 'rocksmith'>,
  inputs: readonly Pick<InputDevice, 'name' | 'rocksmith'>[],
  chosen: boolean,
  tried: string | null,
): string | null {
  if ((!s.ready && !s.error) || (s.ready && s.rocksmith) || chosen) return null;
  const cable = inputs.find((i) => i.rocksmith);
  return cable && cable.name !== tried ? cable.name : null;
}

export interface HwHudHooks {
  /** Real guitar (false) or the game playing itself (true). */
  setAutopilot(on: boolean): void;
  openTuner(): void;
  openAudio(): void;
  openImports(): void;
}

/**
 * The hardware strip at the top right of the menus: the input listened to and whether it is
 * connected, the latency measured, a live level meter, the real guitar / demo switch and the
 * tuner. In the browser (no audio engine) it says so and offers the demo only.
 */
export class HwHud {
  private readonly el: Record<string, HTMLElement> = {};
  private readonly segments: HTMLElement[] = [];
  private shown = -Infinity;
  private hold = { db: -Infinity, at: 0 };

  constructor(
    private readonly root: HTMLElement,
    private readonly live: boolean,
    private readonly hooks: HwHudHooks,
  ) {
    root.innerHTML = `
      <button class="hw-import" data-hw="imports" data-el="imports" title="${t('File d’import : ouvrir')}" hidden>
        <b>Import</b><span data-el="importCount"></span><i class="hw-import-bar"><i data-el="importBar"></i></i>
      </button>
      <button class="hw-source" data-hw="audio" title="${t('Réglages audio')}">
        <i class="hw-status" data-el="led"></i>
        <span class="hw-source-text"><b data-el="device">${t('Démarrage…')}</b><small data-el="state"></small></span>
      </button>
      <span class="hw-meter" data-el="meter" title="${t('Niveau de la guitare')}"></span>
      <button class="hw-mode" data-hw="mode" title="${t('Clic : passer de la guitare réelle à la démo automatique, et inversement')}">
        <span data-mode="real">${t('Guitare réelle')}</span><span data-mode="demo">${t('Démo auto')}</span>
      </button>
      <button class="hw-tuner" data-hw="tuner" title="${t('Accordeur chromatique')}">Tuner</button>`;
    root.querySelectorAll<HTMLElement>('[data-el]').forEach((e) => (this.el[e.dataset.el!] = e));
    for (let i = 0; i < SEGMENTS; i++) {
      const s = document.createElement('i');
      s.className = i >= SEGMENTS - 1 ? 'hot' : i >= SEGMENTS - 3 ? 'warm' : '';
      this.segments.push(s);
      this.el.meter.append(s);
    }
    root.addEventListener('click', (e) => {
      const what = (e.target as HTMLElement).closest<HTMLElement>('[data-hw]')?.dataset.hw;
      if (what === 'mode' && this.live) this.hooks.setAutopilot(!root.classList.contains('demo'));
      else if (what === 'tuner' && this.live) this.hooks.openTuner();
      else if (what === 'audio') this.hooks.openAudio();
      else if (what === 'imports') this.hooks.openImports();
    });
    root.classList.toggle('browser', !live);
    if (!live) {
      this.el.device.textContent = t('Navigateur · pas d’entrée guitare');
      this.el.state.textContent = t('L’app de bureau écoute ta guitare');
      this.setAutopilot(true);
    }
  }

  get hidden(): boolean {
    return this.root.hidden;
  }

  show(on: boolean): void {
    this.root.hidden = !on;
  }

  setAutopilot(on: boolean): void {
    this.root.classList.toggle('demo', on);
  }

  /** What the engine reports: the input, whether it runs, the round trip measured. */
  status(s: AudioStatus | null): void {
    if (!this.live) return;
    const led = this.el.led;
    if (!s || (!s.ready && !s.error)) {
      this.el.device.textContent = t('Moteur audio…');
      this.el.state.textContent = t('Démarrage');
      led.className = 'hw-status wait';
      return;
    }
    if (s.error || !s.ready) {
      this.el.device.textContent = t('Aucune entrée audio');
      this.el.state.textContent = s.error ?? t('Moteur arrêté');
      led.className = 'hw-status off';
      return;
    }
    this.el.device.textContent = s.input ?? t('Entrée audio');
    const latency = s.roundTripMs !== null ? `${Math.round(s.roundTripMs)} ms` : '— ms';
    this.el.state.textContent = t('{input} · connecté · {latency}', { input: s.rocksmith ? t('Câble guitare USB') : t('Entrée audio'), latency });
    led.className = `hw-status ${s.rocksmith ? 'on' : 'warn'}`;
  }

  /** The import queue in the background: how far it got, while songs keep landing. */
  imports(s: QueueSummary): void {
    const busy = s.waiting > 0 || s.running !== null;
    this.el.imports.hidden = !busy;
    if (!busy) return;
    this.el.imports.classList.toggle('paused', s.paused);
    this.el.importCount.textContent = s.paused ? t('en pause · {done}/{total}', { done: s.finished, total: s.total }) : `${s.finished}/${s.total}`;
    this.el.importBar.style.width = `${Math.round(s.fraction * 100)}%`;
    this.el.imports.title = s.running
      ? t('Import en cours : {song} · {n} en file', { song: s.running.title ?? s.running.name, n: s.waiting })
      : t('{n} en file', { n: s.waiting });
  }

  /** The guitar's loudest sample since the last frame. */
  level(peak: number, nowMs: number): void {
    const db = toDb(peak);
    this.shown = Math.max(db, this.shown - 0.6);
    if (db >= this.hold.db || nowMs - this.hold.at > 1200) this.hold = { db, at: nowMs };
    const lit = Math.round(((Math.max(-60, Math.min(0, this.shown)) + 60) / 60) * SEGMENTS);
    const held = Math.round(((Math.max(-60, Math.min(0, this.hold.db)) + 60) / 60) * SEGMENTS) - 1;
    this.segments.forEach((s, i) => {
      s.classList.toggle('on', i < lit);
      s.classList.toggle('held', i === held && held >= lit);
    });
  }
}
