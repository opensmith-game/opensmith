/**
 * The band's gear, bought in the shop with the career's money and kept for the career (a new
 * one starts without): each piece changes the gigs.
 */

import { t } from '../i18n';

export type GearId = 'guitare2' | 'merch' | 'ampli' | 'sono' | 'van';

export interface Gear {
  id: GearId;
  name: string;
  blurb: string;
  price: number;
}

export const GEAR: readonly Gear[] = [
  {
    id: 'guitare2',
    name: t('Deuxième guitare'),
    blurb: t('Accordée d’avance : passer de l’une à l’autre entre deux morceaux ne refroidit plus la salle (deux accordages par concert).'),
    price: 350,
  },
  {
    id: 'merch',
    name: t('Stand de merch'),
    blurb: t('T-shirts et badges à chaque concert : de l’argent en plus, selon le public et sa satisfaction.'),
    price: 500,
  },
  {
    id: 'ampli',
    name: t('Ampli à lampes'),
    blurb: t('Un son qui claque dès la première note : le public démarre 8 points plus chaud.'),
    price: 700,
  },
  {
    id: 'sono',
    name: t('Sono de tournée'),
    blurb: t('Plus de puissance, plus de monde : 15 % de public en plus à chaque date.'),
    price: 1100,
  },
  {
    id: 'van',
    name: t('Le van'),
    blurb: t('Le groupe et le matos sur la route : deux concerts par semaine au lieu d’un.'),
    price: 1500,
  },
];

export const isGear = (id: unknown): id is GearId => GEAR.some((g) => g.id === id);

/** What the tube amp adds to the crowd at the start of a gig. */
export const AMP_WARMTH = 0.08;
/** The audience the tour PA adds. */
export const PA_AUDIENCE = 1.15;

/** Merch sold at a gig (€, by 5): about 60 cents a head in a room on fire. */
export const merchSales = (audience: number, satisfaction: number): number => Math.round((audience * satisfaction * 0.6) / 5) * 5;
