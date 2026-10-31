import { t } from './i18n';
import { type Lesson, type LessonRecords, nextLesson, passed } from './lessons';

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = ''): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

const pct = (x: number): string => t('{n} %', { n: Math.round(x * 100) });

/** The lessons screen: every lesson, passed or not; the one picked (the next to pass at first)
 * opens on how to play it. */
export class LessonsScreen {
  private list: Lesson[] = [];
  private records: LessonRecords = {};
  private picked = 0;

  constructor(
    private readonly root: HTMLElement,
    /** Plays lesson `index`. */
    private readonly start: (index: number) => void,
  ) {
    root.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      const go = target.closest<HTMLElement>('[data-lesson-start]');
      if (go) {
        this.start(Number(go.dataset.lessonStart));
        return;
      }
      const card = target.closest<HTMLElement>('[data-lesson]');
      if (card) {
        this.picked = Number(card.dataset.lesson);
        this.render();
      }
    });
  }

  /** The lessons with their records, `picked` opened (by default the next one to pass). */
  show(list: Lesson[], records: LessonRecords, picked = nextLesson(list, records)): void {
    this.list = list;
    this.records = records;
    this.picked = picked;
    this.render();
  }

  /** Plays the lesson opened (Enter). */
  startPicked(): void {
    this.start(this.picked);
  }

  private render(): void {
    const done = this.list.filter((l) => passed(l, this.records)).length;
    const taught = this.list.filter((l) => !l.record).length;
    const next = nextLesson(this.list, this.records);
    const panel = el('div', 'panel lessons-panel');
    const head = el('header', 'screen-head');
    const back = el('button', 'action small', t('← Menu'));
    back.type = 'button';
    back.dataset.action = 'menu';
    head.append(back, el('h2', '', t('Leçons')), el('span', 'lessons-count', t('{done} sur {total} réussies', { done, total: taught })));
    panel.append(
      head,
      el('p', 'help', t('Pour commencer de zéro. Chaque leçon dure moins d’une minute, sur un métronome : atteins l’objectif pour passer à la suivante.')),
    );
    let list = el('ol', 'lesson-list');
    this.list.forEach((lesson, i) => {
      if (lesson.record && !this.list[i - 1]?.record) {
        // The recording sessions, apart: they teach nothing, they let the game learn the guitar.
        panel.append(list);
        list = el('ol', 'lesson-list');
        panel.append(
          el('h3', 'lessons-record-head', t('Ta guitare, pour mieux t’écouter')),
          el('p', 'help', t('Deux séances de quelques minutes, sans objectif : ta guitare est enregistrée sur ce Mac pendant que tu joues, pour régler la détection des notes sur son son.')),
        );
      }
      const best = this.records[lesson.id];
      const ok = passed(lesson, this.records);
      const item = el('li', `lesson${ok ? ' passed' : ''}${i === next && !ok ? ' next' : ''}${i === this.picked ? ' open' : ''}`);
      const top = el('button', 'lesson-top');
      top.type = 'button';
      top.dataset.lesson = String(i);
      const text = el('span', 'lesson-text');
      text.append(el('b', '', lesson.title), el('span', '', lesson.learn));
      const meta = lesson.record
        ? t('{n} enregistrements · {minutes} min', { n: lesson.steps.length, minutes: Math.ceil(((lesson.steps[lesson.steps.length - 1].beat + 8) * 60) / lesson.bpm / 60) })
        : best === undefined
          ? t('Objectif {goal}', { goal: pct(lesson.goal) })
          : t('Objectif {goal} · ton meilleur {best}', { goal: pct(lesson.goal), best: pct(best) });
      text.append(el('small', '', meta));
      top.append(el('span', 'lesson-number', lesson.record ? '●' : ok ? '✓' : String(i + 1)), text);
      item.append(top);
      if (i === this.picked) {
        const how = el('div', 'lesson-how');
        how.append(el('h3', '', t('Comment faire')));
        const steps = el('ul');
        steps.append(...lesson.how.map((h) => el('li', '', h)));
        const go = el('button', 'action primary', lesson.record ? t('Commencer l’enregistrement') : ok ? t('Rejouer la leçon') : t('Commencer la leçon'));
        go.type = 'button';
        go.dataset.lessonStart = String(i);
        how.append(steps, el('p', 'lesson-tempo', t('Tempo : {bpm} battements par minute', { bpm: lesson.bpm })), go);
        item.append(how);
      }
      list.append(item);
    });
    panel.append(list);
    this.root.replaceChildren(panel);
    this.root.querySelector<HTMLElement>('.lesson.open')?.scrollIntoView({ block: 'nearest' });
  }
}
