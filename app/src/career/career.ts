/**
 * Career mode: from the garage to the stadium (phase 1: the garage and the bars).
 *
 * A career is a save: the season and week, the fans, the money, the act. A week has three slots:
 * a gig (two with the van), a band rehearsal, rest. Every week the venues of the act offer a few
 * gigs; a gig is a setlist played song after song in front of a crowd,
 * whose satisfaction makes the fans won and the fee, scaled by how demanding the songs played
 * were (notes per minute, at the level played). A crowd still on fire at the end calls the band
 * back for an encore, paid on top. Each act ends with a decisive gig, open once enough fans
 * follow: a crowd won over there leads to the next act. A season of 12 weeks ends with its
 * report. Only songs of the band's repertoire go on stage, their form (rehearsed or rusty)
 * weighing on fans and fee, as do the musicians around the player and the gear bought. Every gig leaves a souvenir, its poster, in the album. The money buys guitars in the
 * shop; the band, guitars, trophies and their counts, and the album stay, even through a new
 * career.
 */

import { sanitizeBand, type Band } from '../band';
import { DEFAULT_GUITAR, GUITARS, guitarOf, isGuitar, type Guitar } from '../guitars';
import type { SceneryId } from '../highway/scenery';
import { lang, t } from '../i18n';
import { gradeAtLeast, gradeFor } from '../progress';
import { isTrophy } from '../trophies';
import { isDifficulty, levelOf, type Difficulty } from './difficulty';
import { headlinerOf, RIVAL, RIVAL_TARGET, stolenDate, weekEvent } from './events';
import { AMP_WARMTH, GEAR, isGear, merchSales, PA_AUDIENCE, type GearId } from './gear';
import { ALBUM_TRACKS, albumQuality, albumSales, emptyAlbum, recordTake, sanitizeAlbum, seasonLabel, signed, type Album, type Release } from './label';
import { afterGig, afterWeek, bandFactor, crowdBoost, departures, hire, lineUp, ROLES, sanitizeMembers, startingBand, type Candidate, type Musician } from './musicians';
import { formFactor, formOf, rehearse, rehearseAll, sanitizeRepertoire, STAGE, type Repertoire } from './repertoire';

/** How a venue looks behind the highway: its set, how full, the name on its sign. */
export interface VenueLook {
  scene: SceneryId;
  /** The set's crowd, 1 = as many as it usually holds. */
  fill: number;
  sign?: string;
}

export interface Venue {
  id: string;
  name: string;
  blurb: string;
  act: number;
  /** Audience range. */
  audience: [number, number];
  /** Fee (€) for a crowd on fire; a cold one pays less. */
  fee: number;
  songs: number;
  /** Fans needed to be booked here. */
  fans: number;
  /** The act's decisive gig: won with a satisfaction of at least WIN (and `grade`). */
  decisive?: boolean;
  /** The gig's grade (its songs' accuracy) the decisive gig asks for too. */
  grade?: string;
  /** An opening act: the band the crowd came for. */
  opening?: string;
  /** The decisive gig's satisfaction to reach (WIN by default); against the rival band. */
  win?: number;
  rival?: boolean;
  /** The last decisive gig: the Hall of Fame. */
  final?: boolean;
  look: VenueLook;
}

export const ACTS: Record<number, { name: string; blurb: string }> = {
  1: { name: t('Le garage'), blurb: t('Les copains, les fêtes, les premières fois devant du monde.') },
  2: { name: t('Les bars'), blurb: t('Les premières scènes payées, des publics à gagner.') },
  3: { name: t('Les clubs'), blurb: t('Des salles de 300 à 800 places, des premières parties, et un label qui tend l’oreille.') },
  4: { name: t('Les festivals'), blurb: t('Les scènes d’été, de 5 000 à 20 000 personnes, et Les Vautours sur ta route jusqu’au tremplin.') },
  5: { name: t('Les stades'), blurb: t('La tournée en tête d’affiche : 40 000 personnes et plus, jusqu’à la dernière date.') },
  6: { name: t('Le Hall of Fame'), blurb: t('La tournée est finie, la légende commence : les stades te réclament encore.') },
};

/** The last act; beyond it (the Hall of Fame), the career goes on in the stadiums. */
export const LAST_ACT = 5;

export const VENUES: readonly Venue[] = [
  {
    id: 'garage',
    name: t('Le garage de Max'),
    blurb: t('Les copains sur des caisses de bière, la répète ouverte du samedi.'),
    act: 1,
    audience: [5, 12],
    fee: 0,
    songs: 2,
    fans: 0,
    look: { scene: 'garage', fill: 1 },
  },
  {
    id: 'anniversaire',
    name: t('L’anniversaire de Léa'),
    blurb: t('Un salon, des guirlandes et des amis qui chantent faux.'),
    act: 1,
    audience: [12, 25],
    fee: 20,
    songs: 2,
    fans: 10,
    look: { scene: 'fete', fill: 1 },
  },
  {
    id: 'quartier',
    name: t('La fête de quartier'),
    blurb: t('Une scène en palettes au milieu de la rue.'),
    act: 1,
    audience: [20, 30],
    fee: 40,
    songs: 3,
    fans: 20,
    look: { scene: 'rue', fill: 1 },
  },
  {
    id: 'lycee',
    name: t('La fête de fin d’année'),
    blurb: t('Tout le lycée dans le gymnase. Si la salle suit, les bars t’ouvrent leurs portes.'),
    act: 1,
    audience: [60, 90],
    fee: 80,
    songs: 3,
    fans: 50,
    decisive: true,
    look: { scene: 'gymnase', fill: 1 },
  },
  {
    id: 'saloon',
    name: t('Le Saloon'),
    blurb: t('Scène ouverte du mardi : trois morceaux et au suivant.'),
    act: 2,
    audience: [50, 90],
    fee: 60,
    songs: 3,
    fans: 0,
    look: { scene: 'bar', fill: 0.6, sign: 'SALOON' },
  },
  {
    id: 'zinc',
    name: t('Le Zinc'),
    blurb: t('Le bar rock du centre-ville, ses habitués et son patron exigeant.'),
    act: 2,
    audience: [80, 130],
    fee: 100,
    songs: 4,
    fans: 150,
    look: { scene: 'bar', fill: 0.8, sign: t('LE ZINC') },
  },
  {
    id: 'pub',
    name: t('Le Pub du port'),
    blurb: t('Marins, étudiants et bière tiède : un public qui se mérite.'),
    act: 2,
    audience: [100, 150],
    fee: 140,
    songs: 4,
    fans: 300,
    look: { scene: 'bar', fill: 1, sign: t('PUB DU PORT') },
  },
  {
    id: 'bistrot',
    name: t('Le Bistrot Rock'),
    blurb: t('Le patron cherche un groupe pour tous ses vendredis : ton premier vrai cachet.'),
    act: 2,
    audience: [150, 200],
    fee: 300,
    songs: 5,
    fans: 500,
    decisive: true,
    look: { scene: 'bar', fill: 1.3, sign: t('BISTROT ROCK') },
  },
  {
    id: 'soussol',
    name: t('Le Sous-Sol'),
    blurb: t('Un club en sous-sol, un mur de LED et un public qui vient pour découvrir.'),
    act: 3,
    audience: [300, 400],
    fee: 400,
    songs: 4,
    fans: 0,
    look: { scene: 'club', fill: 0.7, sign: t('LE SOUS-SOL') },
  },
  {
    id: 'usine',
    name: t('L’Usine'),
    blurb: t('Première partie de Neon Wolves : 700 personnes venues pour eux, à conquérir en trois morceaux.'),
    act: 3,
    audience: [600, 750],
    fee: 250,
    songs: 3,
    fans: 1000,
    opening: 'Neon Wolves',
    look: { scene: 'club', fill: 1.6, sign: t('L’USINE') },
  },
  {
    id: 'neonclub',
    name: t('Le Néon Club'),
    blurb: t('La salle branchée du moment : les programmateurs y passent le vendredi.'),
    act: 3,
    audience: [400, 550],
    fee: 700,
    songs: 5,
    fans: 1800,
    look: { scene: 'club', fill: 1, sign: t('NÉON CLUB') },
  },
  {
    id: 'machine',
    name: t('La Machine'),
    blurb: t('Le club mythique de la ville : y jouer complet, c’est entrer dans la cour des grands.'),
    act: 3,
    audience: [550, 700],
    fee: 1000,
    songs: 5,
    fans: 3000,
    look: { scene: 'club', fill: 1.3, sign: t('LA MACHINE') },
  },
  {
    id: 'showcase',
    name: t('Le showcase du label'),
    blurb: t('Un dénicheur de label au Paradiso : la salle doit suivre, et le jeu doit être propre (note A).'),
    act: 3,
    audience: [700, 800],
    fee: 1500,
    songs: 6,
    fans: 4500,
    decisive: true,
    grade: 'A',
    look: { scene: 'club', fill: 1.5, sign: 'PARADISO' },
  },
  {
    id: 'plage',
    name: t('Le Festival de la Plage'),
    blurb: t('Une scène les pieds dans le sable, le premier grand festival de l’été.'),
    act: 4,
    audience: [5000, 7000],
    fee: 2500,
    songs: 5,
    fans: 0,
    look: { scene: 'festival', fill: 0.7, sign: t('FESTIVAL DE LA PLAGE') },
  },
  {
    id: 'dunes',
    name: t('Les Dunes Électriques'),
    blurb: t('Trois jours de rock dans les dunes, une scène principale qui se mérite.'),
    act: 4,
    audience: [8000, 12000],
    fee: 4000,
    songs: 6,
    fans: 8000,
    look: { scene: 'festival', fill: 0.9, sign: t('DUNES ÉLECTRIQUES') },
  },
  {
    id: 'pierres',
    name: t('Les Vieilles Pierres'),
    blurb: t('Le festival du château : la scène où les groupes deviennent grands.'),
    act: 4,
    audience: [12000, 16000],
    fee: 6000,
    songs: 6,
    fans: 12000,
    look: { scene: 'festival', fill: 1.1, sign: t('VIEILLES PIERRES') },
  },
  {
    id: 'tremplin',
    name: t('Le tremplin'),
    blurb: t('Les Vautours et toi, la même scène, le même soir : le meilleur des deux part en tournée des stades.'),
    act: 4,
    audience: [18000, 20000],
    fee: 8000,
    songs: 6,
    fans: 15000,
    decisive: true,
    rival: true,
    win: RIVAL_TARGET,
    look: { scene: 'festival', fill: 1.4, sign: t('LE TREMPLIN') },
  },
  {
    id: 'sud',
    name: t('Le Grand Stade du Sud'),
    blurb: t('La première date de la tournée : 40 000 personnes qui chantent avant même la première note.'),
    act: 5,
    audience: [40000, 50000],
    fee: 15000,
    songs: 7,
    fans: 0,
    look: { scene: 'stade', fill: 0.8, sign: t('STADE DU SUD') },
  },
  {
    id: 'capitale',
    name: t('Le Stade de la Capitale'),
    blurb: t('Deux soirs complets dans la capitale : la presse entière est là.'),
    act: 5,
    audience: [60000, 70000],
    fee: 25000,
    songs: 8,
    fans: 60000,
    look: { scene: 'stade', fill: 1, sign: t('STADE DE LA CAPITALE') },
  },
  {
    id: 'londres',
    name: t('Le stade de Londres'),
    blurb: t('La tournée passe la Manche : 80 000 voix dans un stade mythique.'),
    act: 5,
    audience: [75000, 85000],
    fee: 40000,
    songs: 8,
    fans: 120000,
    look: { scene: 'stade', fill: 1.2, sign: 'LONDON' },
  },
  {
    id: 'derniere',
    name: t('La dernière date'),
    blurb: t('Le dernier concert de la tournée, filmé pour l’histoire : public à 75 % et note A pour entrer au Hall of Fame.'),
    act: 5,
    audience: [90000, 90000],
    fee: 60000,
    songs: 8,
    fans: 180000,
    decisive: true,
    final: true,
    win: 0.75,
    grade: 'A',
    look: { scene: 'stade', fill: 1.4, sign: t('DERNIÈRE DATE') },
  },
];

export const WEEKS = 12;
/** Satisfaction that wins a decisive gig. */
export const WIN = 0.6;
/** The crowd at the end of the setlist that calls the band back for an encore. */
export const ENCORE = 0.8;
/** A setlist lasts at least this long per song (s): no gig of short snippets. */
export const MIN_SONG = 150;
/** Notes per minute (judged, at the level played) worth a reward factor of 1. */
const STEADY = 120;
const OFFERS = 3;
/** Slots of a week, and what a band rehearsal brings every song's form. */
export const SLOTS = 3;
const BAND_REHEARSAL = 0.15;
/** News kept for the hub. */
const NEWS = 5;
const HISTORY = 40;
/** Souvenirs the album keeps, the newest first. */
const ALBUM = 300;
const STORAGE = 'opensmith.career';

export type GigOutcome = 'played' | 'booed' | 'abandoned';

export interface GigSong {
  id: string;
  title: string;
  grade: string;
  /** Notes played among those judged, 0..1. */
  accuracy: number;
  /** The crowd when the song ended, 0..1. */
  crowd: number;
  /** Notes and chords judged, and seconds played (the whole song unless cut short). */
  notes: number;
  seconds: number;
}

/** An encore played to the end, and what it paid on top. */
export interface Encore {
  title: string;
  crowd: number;
  fans: number;
  money: number;
}

export interface GigRecord {
  /** The venue's name, in the interface language (see `sanitizeCareer`), and its id (saves of
   * before the ids: none). */
  venue: string;
  venueId?: string;
  season: number;
  week: number;
  outcome: GigOutcome;
  satisfaction: number;
  /** How demanding the songs played were: the factor on fans and fee (0.6 to 1.4). */
  effort: number;
  /** How rehearsed the songs were: the factor on fans and fee (0.85 to 1.1). */
  form: number;
  /** How precise the band of the night was: the factor on fans and fee; who missed it. */
  band: number;
  missing: string[];
  /** Merch sold (€, in `money`). */
  merch: number;
  /** Pauses taken during the gig. */
  pauses: number;
  /** The setlist's accuracy (by notes) and grade. */
  accuracy: number;
  grade: string;
  /** With the encore's. */
  fans: number;
  money: number;
  songs: GigSong[];
  encore: Encore | null;
  /** In the interface language: written again from the gig when the save is read. */
  review: string;
}

/** A gig's poster in the album: the band as it looked then, the venue, the night, the songs. */
export interface Souvenir {
  /** The venue's id, and its name in the interface language (see `sanitizeCareer`). */
  venue: string;
  name: string;
  act: number;
  decisive: boolean;
  /** The headliner of an opening act. */
  opening?: string;
  season: number;
  week: number;
  /** When it was played (ISO). */
  date: string;
  band: Band | null;
  audience: number;
  setlist: string[];
  encore: string | null;
  outcome: GigOutcome;
  satisfaction: number;
}

/** Over every career: what some trophies count. */
export interface CareerStats {
  /** Gigs played to the end, those of them without a single pause, encores played. */
  gigs: number;
  cleanGigs: number;
  encores: number;
  /** Fees earned (€). */
  earned: number;
}

export interface Career {
  version: 1;
  season: number;
  week: number;
  act: number;
  fans: number;
  money: number;
  /** Newest first. */
  history: GigRecord[];
  /** A season just over whose report is still to be read. */
  report: number | null;
  /** Guitars bought in the shop or won, and the one played (in every mode). */
  guitars: string[];
  guitar: string;
  /** Trophy id → when it was earned (ISO date). */
  trophies: Record<string, string>;
  stats: CareerStats;
  /** The songs the band knows, and their form (see repertoire.ts); empty until first seeded. */
  repertoire: Repertoire;
  /** This week's slots used: 'gig:<venue id>', 'rehearsal', 'rest'. */
  slots: string[];
  /** Gear bought for this career. */
  gear: GearId[];
  /** The bassist, drummer and singer (see musicians.ts). */
  members: Musician[];
  /** What happened lately, the newest first (members leaving…). */
  news: string[];
  /** How demanding the career is (see difficulty.ts). */
  difficulty: Difficulty;
  /** Takes for the next album, and the albums released (see label.ts). */
  album: Album;
  /** The label's word on the last season over, for its report. */
  labelReport: { season: number; met: number; bonus: number } | null;
  /** Careers that won the last date: kept through every new one. */
  hallOfFame: HallEntry[];
  /** The weekly challenge's best tier this week (see challenge.ts). */
  challenge: { key: string; tier: number } | null;
  /** The player's band, null until named. */
  band: Band | null;
  /** Newest first. */
  souvenirs: Souvenir[];
}

/** A gig on this week's agenda. */
export interface Offer {
  venue: Venue;
  audience: number;
  songs: number;
  /** The week's event at this date: a scout in the room (fans ×2), a power cut (acoustic set),
   * a cancelled headliner replaced (twice the audience and fee, a colder crowd). */
  scout?: boolean;
  acoustic?: boolean;
  headliner?: string;
  /** Taken by the rival band this week. */
  stolen?: boolean;
}

/** One who made it: the band, how, when. */
export interface HallEntry {
  band: string;
  difficulty: Difficulty;
  season: number;
  fans: number;
  date: string;
}

export function newCareer(difficulty: Difficulty = 'pro'): Career {
  return {
    version: 1,
    season: 1,
    week: 1,
    act: 1,
    fans: 0,
    money: 0,
    history: [],
    report: null,
    guitars: [],
    guitar: DEFAULT_GUITAR,
    trophies: {},
    stats: { gigs: 0, cleanGigs: 0, encores: 0, earned: 0 },
    repertoire: {},
    slots: [],
    gear: [],
    members: startingBand(),
    news: [],
    difficulty,
    album: emptyAlbum(),
    labelReport: null,
    hallOfFame: [],
    challenge: null,
    band: null,
    souvenirs: [],
  };
}

/** A career from scratch; the band, guitars, trophies and their counts, and the album stay. */
export function resetCareer(c: Career, difficulty: Difficulty = c.difficulty): Career {
  return {
    ...newCareer(difficulty),
    guitars: c.guitars,
    guitar: c.guitar,
    trophies: c.trophies,
    stats: c.stats,
    band: c.band,
    souvenirs: c.souvenirs,
    hallOfFame: c.hallOfFame,
  };
}

const isSouvenir = (s: unknown): s is Souvenir => {
  const x = s as Souvenir;
  return !!x && typeof x.name === 'string' && typeof x.date === 'string' && Array.isArray(x.setlist);
};

const num = (x: unknown, lo: number, hi: number, fallback: number): number =>
  typeof x === 'number' && Number.isFinite(x) ? Math.min(hi, Math.max(lo, Math.round(x))) : fallback;

/**
 * A gig of a save in the interface language: its venue's name, and its review written again from
 * the gig (only when the save kept all it was written from). A save from before the venue ids
 * finds the venue by its name, in French.
 */
function localizeRecord(saved: Partial<GigRecord>, r: GigRecord): GigRecord {
  const venue = VENUES.find((v) => v.id === saved.venueId) ?? VENUES.find((v) => v.name === t(r.venue));
  if (!venue) return r;
  const known = typeof saved.review === 'string' && typeof saved.outcome === 'string' && typeof saved.grade === 'string' && Array.isArray(saved.songs);
  return { ...r, venue: venue.name, venueId: venue.id, ...(known ? { review: reviewOf(venue, r) } : {}) };
}

/** A save read back: from an older version or tampered with, back into shape; null if none. */
export function sanitizeCareer(raw: unknown): Career | null {
  if (!raw || typeof raw !== 'object') return null;
  const c = raw as Partial<Career>;
  const guitars = Array.isArray(c.guitars) ? [...new Set(c.guitars.filter((id) => isGuitar(id) && id !== DEFAULT_GUITAR))] : [];
  const history: GigRecord[] = Array.isArray(c.history)
    ? c.history
        .filter((r) => r && typeof r.venue === 'string')
        .map((r) =>
          localizeRecord(r, {
            ...r,
            effort: typeof r.effort === 'number' ? r.effort : 1,
            encore: r.encore ?? null,
            accuracy: typeof r.accuracy === 'number' ? r.accuracy : 0,
            form: typeof r.form === 'number' ? r.form : 1,
            band: typeof r.band === 'number' ? r.band : 1,
            missing: Array.isArray(r.missing) ? r.missing : [],
            merch: typeof r.merch === 'number' ? r.merch : 0,
            pauses: typeof r.pauses === 'number' ? r.pauses : 0,
            grade: typeof r.grade === 'string' ? r.grade : '—',
          }),
        )
        .slice(0, HISTORY)
    : [];
  const trophies: Record<string, string> = {};
  if (c.trophies && typeof c.trophies === 'object') for (const [id, when] of Object.entries(c.trophies)) if (isTrophy(id) && typeof when === 'string') trophies[id] = when;
  // A save from before the counts: what its history tells.
  const st = (c.stats ?? {}) as Partial<CareerStats>;
  const played = history.filter((r) => r.outcome === 'played');
  const stats: CareerStats = {
    gigs: num(st.gigs, 0, 1e9, played.length),
    cleanGigs: num(st.cleanGigs, 0, 1e9, 0),
    encores: num(st.encores, 0, 1e9, 0),
    earned: num(st.earned, 0, 1e12, history.reduce((t, r) => t + (r.money || 0), 0)),
  };
  return {
    version: 1,
    season: num(c.season, 1, 999, 1),
    week: num(c.week, 1, WEEKS, 1),
    act: num(c.act, 1, LAST_ACT + 1, 1),
    fans: num(c.fans, 0, 1e9, 0),
    money: num(c.money, 0, 1e9, 0),
    history,
    report: typeof c.report === 'number' && c.report >= 1 ? Math.round(c.report) : null,
    guitars,
    guitar: typeof c.guitar === 'string' && (c.guitar === DEFAULT_GUITAR || guitars.includes(c.guitar)) ? c.guitar : DEFAULT_GUITAR,
    trophies,
    stats,
    repertoire: sanitizeRepertoire(c.repertoire),
    slots: Array.isArray(c.slots) ? c.slots.filter((x) => typeof x === 'string').slice(0, SLOTS) : [],
    gear: Array.isArray(c.gear) ? [...new Set(c.gear.filter(isGear))] : [],
    members: sanitizeMembers(c.members),
    news: Array.isArray(c.news) ? c.news.filter((x) => typeof x === 'string').slice(0, NEWS) : [],
    difficulty: isDifficulty(c.difficulty) ? c.difficulty : 'pro',
    album: sanitizeAlbum(c.album),
    labelReport:
      c.labelReport && typeof c.labelReport.season === 'number' && typeof c.labelReport.bonus === 'number'
        ? { season: c.labelReport.season, met: Number(c.labelReport.met) || 0, bonus: c.labelReport.bonus }
        : null,
    hallOfFame: Array.isArray(c.hallOfFame) ? c.hallOfFame.filter((h) => h && typeof h.band === 'string' && typeof h.date === 'string') : [],
    challenge:
      c.challenge && typeof c.challenge.key === 'string' && typeof c.challenge.tier === 'number'
        ? { key: c.challenge.key, tier: Math.min(3, Math.max(0, Math.round(c.challenge.tier))) }
        : null,
    band: sanitizeBand(c.band),
    souvenirs: Array.isArray(c.souvenirs)
      ? c.souvenirs
          .filter(isSouvenir)
          // The venue's name in the interface language.
          .map((x) => ({ ...x, name: VENUES.find((v) => v.id === x.venue)?.name ?? x.name, band: sanitizeBand(x.band), setlist: x.setlist.filter((s) => typeof s === 'string') }))
          .slice(0, ALBUM)
      : [],
  };
}

export const ownsGuitar = (c: Career, id: string): boolean => id === DEFAULT_GUITAR || c.guitars.includes(id);

/** The guitars a career can play, in the shop's order. */
export const ownedGuitars = (c: Career): Guitar[] => GUITARS.filter((g) => ownsGuitar(c, g.id));

/** The guitar `id` bought (and played from now on); null when it is owned already, too dear,
 * or won only with a trophy. */
export function buyGuitar(c: Career, id: string): Career | null {
  const g = guitarOf(id);
  if (g.id !== id || g.trophy || ownsGuitar(c, id) || c.money < g.price) return null;
  return { ...c, money: c.money - g.price, guitars: [...c.guitars, id], guitar: id };
}

/** The band named and drawn (its name trimmed); an empty name leaves it unnamed. */
export function nameBand(c: Career, band: Band): Career {
  return { ...c, band: sanitizeBand(band) };
}

/** The guitar `id` played from now on, if owned. */
export function equipGuitar(c: Career, id: string): Career {
  return ownsGuitar(c, id) ? { ...c, guitar: id } : c;
}

/** Where a career is kept: the library folder in the app, the browser's storage in the demo. */
export interface CareerStore {
  load(): Promise<unknown>;
  save(c: Career): Promise<void>;
}

/** The browser's storage: the demo's store, and where the first version of the app kept it. */
export const localCareer: CareerStore = {
  async load() {
    try {
      return JSON.parse(localStorage.getItem(STORAGE) ?? 'null');
    } catch {
      return null;
    }
  },
  async save(c) {
    try {
      localStorage.setItem(STORAGE, JSON.stringify(c));
    } catch {
      // storage unavailable: the career lasts for this session
    }
  },
};

/** The act whose venues book the band (the next one is not open yet in this phase). */
export const playingAct = (c: Career): number => Math.min(c.act, LAST_ACT);

/** Small deterministic generator: the same week always brings the same offers. */
function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** This week's gigs: up to three venues of the act open to the band, and the decisive gig once
 * enough fans follow (until it is won). */
export function offers(c: Career): Offer[] {
  const act = playingAct(c);
  const rand = random(c.season * 1009 + c.week * 31 + act * 7);
  const played = new Set(c.slots.filter((s) => s.startsWith('gig:')).map((s) => s.slice(4)));
  const open = VENUES.filter((v) => v.act === act && !v.decisive && c.fans >= v.fans);
  for (let i = open.length - 1; i > 0; i--) {
    const j = Math.floor(rand() * (i + 1));
    [open[i], open[j]] = [open[j], open[i]];
  }
  const picked = open.slice(0, OFFERS).sort((a, b) => a.fans - b.fans);
  const decisive = VENUES.find((v) => v.act === act && v.decisive);
  if (decisive && c.act === act && c.fans >= decisive.fans) picked.push(decisive);
  const pa = c.gear.includes('sono') ? PA_AUDIENCE : 1;
  // Audiences drawn for every venue picked, so a gig played leaves the others' unchanged.
  const list: Offer[] = picked.map((venue) => ({
    venue,
    audience: Math.round((venue.audience[0] + rand() * (venue.audience[1] - venue.audience[0])) * pa),
    songs: venue.songs,
  }));
  // The week's event lands on its first ordinary date; a cancelled headliner adds one.
  const event = weekEvent(c.season, c.week);
  const plain = list.find((o) => !o.venue.decisive);
  if (plain && event === 'denicheur') plain.scout = true;
  if (plain && event === 'panne') plain.acoustic = true;
  if (event === 'annulation') {
    const big = [...open].sort((a, b) => b.fans - a.fans)[0];
    if (big) list.push({ venue: big, audience: Math.round(big.audience[1] * 2 * pa), songs: big.songs, headliner: headlinerOf(c.season, c.week) });
  }
  const stolen = stolenDate(c.season, c.week, list.filter((o) => !o.venue.decisive && !o.headliner).length);
  if (stolen !== null) list.filter((o) => !o.venue.decisive && !o.headliner)[stolen].stolen = true;
  return list.filter((o) => !played.has(o.headliner ? `${o.venue.id}+` : o.venue.id));
}

/** Venues of the act still out of reach, the nearest first: what playing well opens. */
export function lockedVenues(c: Career): Venue[] {
  const act = playingAct(c);
  return VENUES.filter((v) => v.act === act && c.fans < v.fans).sort((a, b) => a.fans - b.fans);
}

/** What the act asks for: the fans that open its decisive gig, then winning it. */
export function objective(c: Career): { text: string; progress: number } {
  const decisive = VENUES.find((v) => v.act === playingAct(c) && v.decisive);
  if (c.act > LAST_ACT || !decisive) {
    return { text: t('{act} : la tournée est finie. Les stades te réclament encore.', { act: ACTS[LAST_ACT + 1].name }), progress: 1 };
  }
  const venue = venueIn(decisive.name);
  if (c.fans < decisive.fans) {
    return { text: t('{n} fans pour jouer {venue}', { n: count(decisive.fans), venue }), progress: c.fans / decisive.fans };
  }
  const vars = { venue, pct: Math.round((decisive.win ?? WIN) * 100), grade: decisive.grade ?? '', rival: RIVAL };
  const grade = decisive.grade ? t(' et une note {grade} ou mieux', vars) : '';
  const rival = decisive.rival ? t(' : fais mieux que {rival}', vars) : '';
  return { text: t('Gagne {venue} : le public à {pct} % ou plus', vars) + grade + rival, progress: 1 };
}

/** A number as the interface language writes it: 4 500, 4,500. */
const count = (n: number): string => n.toLocaleString(lang() === 'en' ? 'en-US' : 'fr-FR');

/** Shortest setlist a gig takes (s). */
export const minSetlist = (offer: Offer): number => offer.songs * MIN_SONG;

/** How demanding a song was as played: 0.6 for a few notes a minute, 1 at STEADY, up to 1.4. */
export function songEffort(s: Pick<GigSong, 'notes' | 'seconds'>): number {
  const perMinute = s.seconds > 0 ? s.notes / (s.seconds / 60) : 0;
  return 0.6 + 0.4 * Math.min(2, perMinute / STEADY);
}

/** A setlist's effort: each song's, weighted by its length. */
export function effortOf(songs: readonly Pick<GigSong, 'notes' | 'seconds'>[]): number {
  const total = songs.reduce((t, s) => t + s.seconds, 0);
  if (total <= 0) return 0.6;
  return songs.reduce((sum, s) => sum + songEffort(s) * s.seconds, 0) / total;
}

/** A setlist's accuracy, each song weighted by its notes judged. */
export function gigAccuracy(songs: readonly Pick<GigSong, 'accuracy' | 'notes'>[]): number {
  const notes = songs.reduce((t, s) => t + s.notes, 0);
  return notes > 0 ? songs.reduce((t, s) => t + s.accuracy * s.notes, 0) / notes : 0;
}

/** Fee for a gig: from 40 % of the venue's for a cold crowd to 120 % for one on fire, times the
 * effort, by 5 €. */
export function feeFor(venue: Venue, satisfaction: number, effort = 1): number {
  return Math.round((venue.fee * (0.4 + 0.8 * satisfaction) * effort) / 5) * 5;
}

/** An encore's bonus: up to 60 % of the venue's fee and a quarter of the audience as new fans,
 * as the crowd ends it and as demanding the song is. */
export function encoreBonus(offer: Offer, song: Pick<GigSong, 'crowd' | 'notes' | 'seconds'>): { fans: number; money: number } {
  const effort = songEffort(song);
  return {
    fans: Math.round(offer.audience * 0.25 * song.crowd * effort),
    money: Math.round((offer.venue.fee * 0.5 * (0.4 + 0.8 * song.crowd) * effort) / 5) * 5,
  };
}

/** Fans won: most of the audience when it loved a demanding show, a few when it did not; a gig
 * cut short loses some. */
export function fansFor(audience: number, satisfaction: number, outcome: GigOutcome, effort = 1): number {
  if (outcome !== 'played') return -Math.round(audience * 0.1);
  return Math.round(audience * (0.2 + satisfaction) * 0.8 * effort);
}

const REVIEWS: [number, (venue: string, song: string) => string][] = [
  [0.85, (venue) => t('« {venue} n’avait pas vu ça depuis longtemps : le public en redemande. »', { venue })],
  [0.7, (_, song) => t('« Une vraie énergie sur scène, {song} a fait lever la salle. »', { song })],
  [0.5, () => t('« Un concert solide, quelques flottements, un public conquis à moitié. »')],
  [0.3, () => t('« Le public est resté poli. Il faudra répéter avant de revenir. »')],
  [0, () => t('« Une soirée difficile : la salle s’est vidée avant la fin. »')],
];

export function reviewFor(venue: Venue, satisfaction: number, outcome: GigOutcome, best: string | null): string {
  if (outcome === 'booed') return t('« Sorti de scène sous les sifflets : {venue} ne t’a pas laissé finir. »', { venue: venue.name });
  if (outcome === 'abandoned') return t('« Le groupe a quitté la scène {ofVenue} en plein concert. »', { ofVenue: ofVenue(venue.name) });
  const line = REVIEWS.find(([at]) => satisfaction >= at)![1];
  return line(venue.name, best ?? t('le premier morceau'));
}

/** The press's word on a gig at `venue`: the rival who did better at the tremplin, the scout or
 * the last date that wanted a cleaner show, else as the crowd followed. */
export function reviewOf(venue: Venue, gig: Pick<GigRecord, 'satisfaction' | 'outcome' | 'grade' | 'songs'>): string {
  const { satisfaction, outcome, grade } = gig;
  const played = outcome === 'played';
  const target = venue.win ?? WIN;
  if (played && venue.rival && satisfaction < target) {
    return t('« {rival} ont mis {target} % du public dans leur poche, toi {pct} % : ils partent en tournée. Retente ta chance. »', {
      rival: RIVAL,
      target: Math.round(target * 100),
      pct: Math.round(satisfaction * 100),
    });
  }
  if (played && venue.grade && satisfaction >= target && !gradeAtLeast(grade, venue.grade)) {
    return venue.final
      ? t('« Un concert immense, mais pas parfait : il fallait une note {need}, pas {grade}, pour entrer dans l’histoire. »', { need: venue.grade, grade })
      : t('« Le dénicheur est reparti sans signer : la salle a suivi, mais il attendait une note {need}, pas {grade}. »', { need: venue.grade, grade });
  }
  const best = [...gig.songs].sort((a, b) => b.crowd - a.crowd)[0]?.title ?? null;
  return reviewFor(venue, satisfaction, outcome, best);
}

/** A venue's name inside a sentence: "jouer la fête de fin d’année", "gagner le Bistrot Rock"
 * (English: "play the end-of-year party"). */
export function venueIn(name: string): string {
  if (lang() === 'en') return name.replace(/^The /, 'the ');
  return name.replace(/^(Le|La|Les|L’) /, (a) => a.toLowerCase()).replace(/^L’/, 'l’');
}

/** "de" and a venue's name: "du garage de Max", "de la fête de quartier", "de l’anniversaire"
 * (English: "of Max’s garage", "of the block party"). */
export function ofVenue(name: string): string {
  const v = venueIn(name);
  if (lang() === 'en') return `of ${v}`;
  if (v.startsWith('le ')) return `du ${v.slice(3)}`;
  if (v.startsWith('les ')) return `des ${v.slice(4)}`;
  return `de ${v}`;
}

/** Weeks since the career began (1 for the first one): the clock of the songs' form. */
export const weekIndex = (c: Pick<Career, 'season' | 'week'>): number => (c.season - 1) * WEEKS + c.week;

/** The week after (12 weeks to a season). */
export function nextWeek(c: Career): Pick<Career, 'season' | 'week'> {
  return c.week >= WEEKS ? { season: c.season + 1, week: 1 } : { season: c.season, week: c.week + 1 };
}

/** The season whose report waits once the week after `c` begins a new one. */
const reportAfter = (c: Career): number | null => (c.week >= WEEKS ? c.season : c.report);

/** Gigs a week allows: one, two with the van. */
export const gigsPerWeek = (c: Pick<Career, 'gear' | 'season' | 'week'>): number =>
  (c.gear.includes('van') ? 2 : 1) - (weekEvent(c.season, c.week) === 'van' ? 1 : 0);

export const slotsLeft = (c: Pick<Career, 'slots'>): number => SLOTS - c.slots.length;

/** The band can still play a gig this week. */
export const canGig = (c: Pick<Career, 'slots' | 'gear' | 'season' | 'week'>): boolean => slotsLeft(c) > 0 && c.slots.filter((s) => s.startsWith('gig')).length < gigsPerWeek(c);

/**
 * The week over (its slots left are rest): the band's morale moves with how the week went, a
 * member left without any goes over to the rival, and the next week begins.
 */
export function restWeek(c: Career): Career {
  const slots = [...c.slots, ...Array.from({ length: slotsLeft(c) }, () => 'rest')];
  const { members, gone } = departures(afterWeek(c.members, slots));
  const label = (m: Musician) => ROLES.find((r) => r.id === m.role)!.label.toLowerCase();
  const news = gone.map((m) => t('{name} quitte le groupe pour {rival} : un remplaçant prend la {part}.', { name: m.name, rival: RIVAL, part: label(m) }));
  // The season over: the label pays the objectives met.
  const verdict = c.week >= WEEKS && signed(c) ? { season: c.season, ...seasonLabel(c, c.season) } : null;
  return {
    ...c,
    ...nextWeek(c),
    report: reportAfter(c),
    slots: [],
    members,
    news: [...news, ...c.news].slice(0, NEWS),
    money: c.money + (verdict?.bonus ?? 0),
    labelReport: verdict ?? c.labelReport,
  };
}

/** A studio take of `id` at `quality` (its accuracy), a slot of the week: the best take of each
 * song is kept for the next album. */
export function studioTake(c: Career, id: string, quality: number): Career {
  if (slotsLeft(c) <= 0) return c;
  return useSlot({ ...c, album: recordTake(c.album, id, quality) }, 'studio');
}

/** The album out (eight songs recorded at least): copies sold by its quality and the band's fans,
 * money and fans from them; the next one starts from scratch. Null when not ready. */
export function releaseAlbum(c: Career, when = new Date()): { career: Career; release: Release } | null {
  if (Object.keys(c.album.takes).length < ALBUM_TRACKS) return null;
  const quality = albumQuality(c.album);
  const copies = albumSales(c.fans, quality);
  const release: Release = { name: `Album ${c.album.released.length + 1}`, season: c.season, quality, copies, date: when.toISOString() };
  return {
    career: {
      ...c,
      money: c.money + copies * 2,
      fans: c.fans + Math.round(copies * 0.15),
      album: { takes: {}, released: [...c.album.released, release] },
    },
    release,
  };
}

/** `slot` used this week; the week ends once all are. */
function useSlot(c: Career, slot: string): Career {
  const next = { ...c, slots: [...c.slots, slot] };
  return slotsLeft(next) > 0 ? next : restWeek(next);
}

/** A slot of rest: morale up at the end of the week. */
export const rest = (c: Career): Career => (slotsLeft(c) > 0 ? useSlot(c, 'rest') : c);

/** A band rehearsal: every song of the repertoire a little more in form. */
export function rehearseBand(c: Career): Career {
  if (slotsLeft(c) <= 0) return c;
  return useSlot({ ...c, repertoire: rehearseAll(c.repertoire, weekIndex(c), BAND_REHEARSAL) }, 'rehearsal');
}

/** The gear `id` bought; null when owned already or too dear. */
export function buyGear(c: Career, id: GearId): Career | null {
  const g = GEAR.find((x) => x.id === id);
  if (!g || c.gear.includes(id) || c.money < g.price) return null;
  return { ...c, money: c.money - g.price, gear: [...c.gear, id] };
}

/** `m` hired in place of the member of the same part; null when too dear or the band has too
 * few fans to attract them. */
export function hireMusician(c: Career, m: Candidate): Career | null {
  if (c.money < m.fee || c.fans < m.fans) return null;
  return { ...c, money: c.money - m.fee, members: hire(c.members, m) };
}

/** The line-up of gig `offer` this week (an unreliable member may miss it: a stand-in plays). */
export function gigLineUp(c: Career, offer: Offer): { members: Musician[]; missing: Musician[] } {
  const night = lineUp(c.members, `${c.season}-${c.week}-${offer.venue.id}`);
  if (weekEvent(c.season, c.week) !== 'malade') return night;
  // The drummer is sick all week.
  const drummer = c.members.find((m) => m.role === 'batterie')!;
  if (night.missing.includes(drummer)) return night;
  return {
    members: night.members.map((m) => (m.role === 'batterie' ? { ...m, name: t('un remplaçant'), precision: 40, energy: 40, reliability: 90, morale: 0.5 } : m)),
    missing: [...night.missing, drummer],
  };
}

/**
 * One song of a setlist in tuning `t` after `prev`, with the tunings the band's guitars hold
 * (`guitars` of them): whether going to it means retuning on stage (the room cools), and what
 * the guitars hold then. A second guitar tuned beforehand takes the second tuning for free.
 */
export function retuneStep(held: readonly string[], prev: string | null, t: string, guitars: number): { held: string[]; costs: boolean } {
  if (held.includes(t)) return { held: [...held], costs: false };
  if (held.length < guitars) return { held: [...held, t], costs: false };
  return { held: held.map((x) => (x === prev ? t : x)), costs: true };
}

/** For each song of a setlist (by tuning), whether going to it means retuning on stage. */
export function retunes(tunings: readonly string[], guitars: number): boolean[] {
  let held: string[] = [];
  return tunings.map((t, i) => {
    const step = retuneStep(held, i ? tunings[i - 1] : null, t, guitars);
    held = step.held;
    return step.costs;
  });
}

/** Guitars on stage: one, two with the second guitar. */
export const guitarsOf = (c: Pick<Career, 'gear'>): number => (c.gear.includes('guitare2') ? 2 : 1);

/** Where the crowd starts a gig: half won (a third for an opening act), warmer with an energetic
 * band and the tube amp. */
export function crowdStart(c: Career, offer: Offer): number {
  // A crowd come for another band starts colder; so does one without power.
  const base = (offer.venue.opening ? 0.3 : 0.5) - (offer.headliner ? 0.15 : 0) - (offer.acoustic ? 0.08 : 0);
  return Math.min(0.9, Math.max(0.1, base + crowdBoost(gigLineUp(c, offer).members) + (c.gear.includes('ampli') ? AMP_WARMTH : 0)));
}

export interface SeasonSummary {
  season: number;
  gigs: number;
  fans: number;
  money: number;
  best: GigRecord | null;
}

/** What a season brought: its gigs, fans, money and best gig (by satisfaction). */
export function seasonSummary(c: Career, season: number): SeasonSummary {
  const gigs = c.history.filter((r) => r.season === season);
  const best = gigs.filter((r) => r.outcome === 'played').sort((a, b) => b.satisfaction - a.satisfaction)[0] ?? null;
  return {
    season,
    gigs: gigs.length,
    fans: gigs.reduce((t, r) => t + r.fans, 0),
    money: gigs.reduce((t, r) => t + r.money, 0),
    best,
  };
}

/**
 * A gig over: its satisfaction (the crowd at the end of each song of the setlist, on average;
 * nothing for a gig cut short), fans and fee with the encore's on top, the act won when it was
 * the decisive gig, the next week, the counts of every career (`pauses` taken during the gig).
 */
export function concludeGig(
  c: Career,
  offer: Offer,
  songs: GigSong[],
  outcome: GigOutcome,
  extra: { encore?: GigSong | null; pauses?: number; when?: Date } = {},
): { career: Career; record: GigRecord; actUp: boolean; souvenir: Souvenir } {
  const satisfaction = songs.length ? songs.reduce((sum, s) => sum + s.crowd, 0) / songs.length : 0;
  const played = outcome === 'played';
  const effort = effortOf(songs);
  // The setlist's form as it went on stage (a song outside the repertoire counts as neutral).
  const now = weekIndex(c);
  const form = formFactor(songs.map((s) => (c.repertoire[s.id] ? formOf(c.repertoire[s.id], now) : 0.6)));
  const bonus = played && extra.encore ? encoreBonus(offer, extra.encore) : null;
  const encore: Encore | null = bonus && extra.encore ? { title: extra.encore.title, crowd: extra.encore.crowd, ...bonus } : null;
  // The band of the night: its precision weighs on fans and fee.
  const night = gigLineUp(c, offer);
  const band = bandFactor(night.members);
  const merch = played && c.gear.includes('merch') ? merchSales(offer.audience, satisfaction) : 0;
  // The career's difficulty pays; a scout in the room doubles the fans, a headliner replaced the fee.
  const reward = levelOf(c.difficulty).reward;
  const fans = Math.round((fansFor(offer.audience, satisfaction, outcome, effort * form * band * reward) + (bonus?.fans ?? 0)) * (offer.scout && played ? 2 : 1));
  const money = (played ? feeFor(offer.venue, satisfaction, effort * form * band * reward * (offer.headliner ? 2 : 1)) : 0) + (bonus?.money ?? 0) + merch;
  // Every song played on stage is back in form.
  let repertoire = c.repertoire;
  for (const s of extra.encore ? [...songs, extra.encore] : songs) repertoire = rehearse(repertoire, s.id, now, STAGE);
  const accuracy = gigAccuracy(songs);
  const grade = gradeFor(accuracy, songs.reduce((t, s) => t + s.notes, 0));
  const clean = !offer.venue.grade || gradeAtLeast(grade, offer.venue.grade);
  const target = offer.venue.win ?? WIN;
  const actUp = played && !!offer.venue.decisive && offer.venue.act === c.act && satisfaction >= target && clean;
  const record: GigRecord = {
    venue: offer.venue.name,
    venueId: offer.venue.id,
    season: c.season,
    week: c.week,
    outcome,
    satisfaction,
    effort,
    form,
    band,
    missing: night.missing.map((m) => m.name),
    merch,
    pauses: extra.pauses ?? 0,
    accuracy,
    grade,
    fans,
    money,
    songs,
    encore,
    review: reviewOf(offer.venue, { satisfaction, outcome, grade, songs }),
  };
  const souvenir: Souvenir = {
    venue: offer.venue.id,
    name: offer.venue.name,
    act: offer.venue.act,
    decisive: !!offer.venue.decisive,
    ...(offer.venue.opening ? { opening: offer.venue.opening } : {}),
    season: c.season,
    week: c.week,
    date: (extra.when ?? new Date()).toISOString(),
    band: c.band,
    audience: offer.audience,
    setlist: songs.map((s) => s.title),
    encore: encore?.title ?? null,
    outcome,
    satisfaction,
  };
  const stats: CareerStats = {
    gigs: c.stats.gigs + (played ? 1 : 0),
    cleanGigs: c.stats.cleanGigs + (played && !extra.pauses ? 1 : 0),
    encores: c.stats.encores + (encore ? 1 : 0),
    earned: c.stats.earned + money,
  };
  // The last date won: into the Hall of Fame.
  const hall: HallEntry[] =
    actUp && offer.venue.final
      ? [...c.hallOfFame, { band: c.band?.name ?? t('Le groupe'), difficulty: c.difficulty, season: c.season, fans: Math.max(0, c.fans + fans), date: (extra.when ?? new Date()).toISOString() }]
      : c.hallOfFame;
  const after: Career = {
    ...c,
    hallOfFame: hall,
    act: actUp ? c.act + 1 : c.act,
    fans: Math.max(0, c.fans + fans),
    money: c.money + money,
    history: [record, ...c.history].slice(0, HISTORY),
    stats,
    repertoire,
    members: afterGig(c.members, satisfaction, outcome),
    souvenirs: [souvenir, ...c.souvenirs].slice(0, ALBUM),
  };
  // A replaced headliner is its own date in the week.
  return { career: useSlot(after, `gig:${offer.venue.id}${offer.headliner ? '+' : ''}`), record, actUp, souvenir };
}
