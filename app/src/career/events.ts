/**
 * What can happen in a week (one event now and then, the same for the whole week) and the rival
 * band, Les Vautours: climbing alongside, taking a date now and then, met at the tremplin.
 */

import { t } from '../i18n';

export type WeekEvent = 'denicheur' | 'panne' | 'annulation' | 'malade' | 'van';

export const EVENTS: Record<WeekEvent, { title: string; text: string }> = {
  denicheur: { title: t('Un dénicheur dans la salle'), text: t('Un dénicheur vient à l’un de tes concerts cette semaine : ce soir-là, les fans comptent double.') },
  panne: { title: t('Panne de courant'), text: t('Le courant saute dans une des salles : set acoustique, l’ampli passe en son clair et le public démarre plus froid.') },
  annulation: { title: t('La tête d’affiche annule'), text: t('Une salle cherche un remplaçant au pied levé : deux fois plus de public et de cachet, mais un public venu pour un autre groupe.') },
  malade: { title: t('Le batteur est malade'), text: t('Toute la semaine, un remplaçant tient la batterie.') },
  van: { title: t('Le van est en panne'), text: t('Un concert de moins cette semaine : la date se rattrapera.') },
};

/** The rival band, its satisfaction to beat at the tremplin. */
export const RIVAL = t('Les Vautours');
export const RIVAL_TARGET = 0.75;

/** Bands an opening slot or a cancelled headliner can be. */
export const HEADLINERS = ['The Blue Riders', t('Sombre Dimanche'), t('Les Orages'), 'Velvet Static'];

function hash(text: string): number {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
  return (h >>> 0) / 4294967296;
}

const ORDER: WeekEvent[] = ['denicheur', 'panne', 'annulation', 'malade', 'van'];

/** The week's event, about one week in three; none in the very first weeks of a career. */
export function weekEvent(season: number, week: number): WeekEvent | null {
  if (season === 1 && week <= 2) return null;
  const r = hash(`event|${season}|${week}`);
  return r < 0.35 ? ORDER[Math.floor((r / 0.35) * ORDER.length)] : null;
}

/** Which of the week's `n` dates the rival took (one week in four, never the only one). */
export function stolenDate(season: number, week: number, n: number): number | null {
  if (n < 2) return null;
  const r = hash(`rival|${season}|${week}`);
  return r < 0.25 ? Math.floor((r / 0.25) * n) : null;
}

/** The rival's fans: close behind the band, a little more every week. */
export const rivalFans = (fans: number, week: number): number => Math.round((fans * 0.9 + week * 25) / 10) * 10;

/** The band a cancelled headliner was, for the week. */
export const headlinerOf = (season: number, week: number): string => HEADLINERS[Math.floor(hash(`headliner|${season}|${week}`) * HEADLINERS.length)];
