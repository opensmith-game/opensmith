/**
 * Game hardware for the form controls: a selector with arrows (`< SALLE DE CONCERT >`) for each
 * <select>, an amp toggle switch with its LED for each checkbox, a notched mixing-desk fader for
 * each range. The native control stays in the page, hidden, as the value the app reads and
 * writes: the hardware follows it (events, values set from code, options replaced) and drives it
 * (the same input and change events the app listens to).
 */

import { t } from '../i18n';

const upgraded = new WeakSet<Element>();

/** Calls `onSet` whenever code sets `prop` on `el` (value, checked, selectedIndex…). */
function watch(el: HTMLElement, prop: string, onSet: () => void): void {
  let proto: object | null = Object.getPrototypeOf(el);
  let desc: PropertyDescriptor | undefined;
  while (proto && !(desc = Object.getOwnPropertyDescriptor(proto, prop))) proto = Object.getPrototypeOf(proto);
  if (!desc?.set || !desc.get) return;
  const { get, set } = desc;
  Object.defineProperty(el, prop, {
    configurable: true,
    get() {
      return get.call(this);
    },
    set(v) {
      set.call(this, v);
      onSet();
    },
  });
}

const fire = (el: HTMLElement) => {
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
};

function selector(select: HTMLSelectElement): void {
  const box = document.createElement('div');
  box.className = 'hw-select';
  box.tabIndex = 0;
  box.setAttribute('role', 'spinbutton');
  const prev = document.createElement('button');
  prev.type = 'button';
  prev.className = 'hw-arrow';
  prev.textContent = '‹';
  prev.setAttribute('aria-label', t('Précédent'));
  prev.tabIndex = -1;
  const next = prev.cloneNode(true) as HTMLButtonElement;
  next.textContent = '›';
  next.setAttribute('aria-label', t('Suivant'));
  const label = document.createElement('span');
  label.className = 'hw-value';
  const dots = document.createElement('span');
  dots.className = 'hw-dots';
  box.append(prev, label, next, dots);
  select.classList.add('hw-native');
  select.tabIndex = -1;
  select.after(box);

  let shown = '';
  const render = (dir = 0) => {
    const option = select.options[select.selectedIndex];
    const text = option?.text ?? '—';
    if (text !== shown) {
      label.textContent = text;
      if (dir && shown) {
        label.classList.remove('from-left', 'from-right');
        void label.offsetWidth;
        label.classList.add(dir > 0 ? 'from-right' : 'from-left');
      }
      shown = text;
    }
    const n = select.options.length;
    if (dots.childElementCount !== n) dots.replaceChildren(...Array.from({ length: Math.min(n, 12) }, () => document.createElement('i')));
    [...dots.children].forEach((d, i) => d.classList.toggle('on', i === Math.min(select.selectedIndex, 11)));
    box.classList.toggle('disabled', select.disabled || n < 2);
  };
  const step = (dir: number) => {
    const n = select.options.length;
    if (n < 2 || select.disabled) return;
    select.selectedIndex = (select.selectedIndex + dir + n) % n;
    render(dir);
    fire(select);
  };
  prev.addEventListener('click', () => step(-1));
  next.addEventListener('click', () => step(1));
  box.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      e.stopPropagation();
      step(e.key === 'ArrowLeft' ? -1 : 1);
    }
  });
  select.addEventListener('change', () => render());
  watch(select, 'value', () => render());
  watch(select, 'selectedIndex', () => render());
  new MutationObserver(() => render()).observe(select, { childList: true, subtree: true, attributes: true, attributeFilter: ['disabled'] });
  render();
}

function toggle(input: HTMLInputElement): void {
  const sw = document.createElement('span');
  sw.className = 'hw-toggle';
  sw.setAttribute('aria-hidden', 'true');
  sw.innerHTML = '<i class="hw-led"></i><span class="hw-lever"><b></b></span>';
  input.classList.add('hw-native');
  input.after(sw);
  // A checkbox outside a label toggles from its switch.
  if (!input.closest('label')) {
    sw.addEventListener('click', () => {
      input.checked = !input.checked;
      fire(input);
    });
  }
  const render = () => sw.classList.toggle('on', input.checked);
  input.addEventListener('change', render);
  input.addEventListener('input', render);
  watch(input, 'checked', render);
  render();
}

function fader(input: HTMLInputElement): void {
  const box = document.createElement('div');
  box.className = 'hw-fader';
  box.tabIndex = 0;
  box.setAttribute('role', 'slider');
  const track = document.createElement('span');
  track.className = 'hw-track';
  const fill = document.createElement('span');
  fill.className = 'hw-fill';
  const notches = document.createElement('span');
  notches.className = 'hw-notches';
  for (let i = 0; i <= 10; i++) notches.append(document.createElement('i'));
  const cap = document.createElement('span');
  cap.className = 'hw-cap';
  track.append(fill, notches, cap);
  box.append(track);
  input.classList.add('hw-native');
  input.tabIndex = -1;
  input.after(box);

  const min = () => Number(input.min || 0);
  const max = () => Number(input.max || 100);
  const stepOf = () => Number(input.step || 1) || 1;
  const render = () => {
    const k = (Number(input.value) - min()) / (max() - min() || 1);
    box.style.setProperty('--k', String(Math.min(1, Math.max(0, k))));
    box.setAttribute('aria-valuenow', input.value);
  };
  const setFrom = (clientX: number) => {
    const r = track.getBoundingClientRect();
    const k = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
    const raw = min() + k * (max() - min());
    const v = Math.round((raw - min()) / stepOf()) * stepOf() + min();
    if (String(v) === input.value) return;
    input.value = String(v);
    render();
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  box.addEventListener('pointerdown', (e) => {
    if (input.disabled) return;
    box.setPointerCapture(e.pointerId);
    box.classList.add('grab');
    setFrom(e.clientX);
  });
  box.addEventListener('pointermove', (e) => {
    if (box.hasPointerCapture(e.pointerId)) setFrom(e.clientX);
  });
  const release = (e: PointerEvent) => {
    if (!box.hasPointerCapture(e.pointerId)) return;
    box.releasePointerCapture(e.pointerId);
    box.classList.remove('grab');
    input.dispatchEvent(new Event('change', { bubbles: true }));
  };
  box.addEventListener('pointerup', release);
  box.addEventListener('pointercancel', release);
  box.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    e.stopPropagation();
    const v = Math.min(max(), Math.max(min(), Number(input.value) + (e.key === 'ArrowLeft' ? -1 : 1) * stepOf()));
    input.value = String(v);
    render();
    fire(input);
  });
  input.addEventListener('input', render);
  watch(input, 'value', render);
  render();
}

function upgrade(el: Element): void {
  if (upgraded.has(el) || el.closest('[data-native]')) return;
  if (el instanceof HTMLSelectElement && !el.multiple) selector(el);
  else if (el instanceof HTMLInputElement && el.type === 'checkbox') toggle(el);
  else if (el instanceof HTMLInputElement && el.type === 'range') fader(el);
  else return;
  upgraded.add(el);
}

const CONTROLS = 'select, input[type="checkbox"], input[type="range"]';

/** Upgrades every control in the page now and every one added later. */
export function hardwareControls(root: HTMLElement = document.body): void {
  root.querySelectorAll(CONTROLS).forEach(upgrade);
  new MutationObserver((records) => {
    for (const r of records) {
      r.addedNodes.forEach((n) => {
        if (!(n instanceof Element)) return;
        if (n.matches(CONTROLS)) upgrade(n);
        n.querySelectorAll(CONTROLS).forEach(upgrade);
      });
    }
  }).observe(root, { childList: true, subtree: true });
}
