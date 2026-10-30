/**
 * Geometry of the note highway, in world units.
 *
 * x runs along the neck: fret lanes of equal width, fret f between wires f-1 and f.
 * y is height: the six strings are stacked at the strike line, notes travel at their string's
 * height with a stem down to the floor of the lane. z is time: the strike line is z = 0 and a
 * note `dt` seconds away sits at z = -dt * speed, so notes come from the distance toward the
 * player.
 */
import type { Anchor } from './chart';

export const FRETS = 24;
export const LANE = 1;
export const STRING_GAP = 0.24;
export const STRING_BASE = 0.26;
export const TOP_Y = STRING_BASE + 5 * STRING_GAP;

export interface Zone {
  /** First and last fret lanes, inclusive. */
  lo: number;
  hi: number;
}

export function clamp(x: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, x));
}

/**
 * Row of a string, 0 = bottom. By default the low E (string 0, red) is on top, like the
 * guitar seen by its player; inverted is tablature order, low E at the bottom.
 */
export function stringRow(string: number, invert: boolean): number {
  return invert ? string : 5 - string;
}

export function stringY(string: number, invert: boolean): number {
  return STRING_BASE + stringRow(string, invert) * STRING_GAP;
}

export function fretWireX(fret: number): number {
  return fret * LANE;
}

export function laneCenter(fret: number): number {
  return (fret - 0.5) * LANE;
}

export function timeZ(time: number, now: number, speed: number): number {
  return -(time - now) * speed;
}

export function anchorZone(a: Anchor): Zone {
  const lo = clamp(a.fret, 1, FRETS);
  return { lo, hi: clamp(a.fret + Math.max(1, a.width) - 1, lo, FRETS) };
}

export function zoneLeft(z: Zone): number {
  return (z.lo - 1) * LANE;
}

export function zoneRight(z: Zone): number {
  return z.hi * LANE;
}

/** Frets the camera must show: the anchor, widened to every fretted note coming soon. */
export function focusZone(anchor: Zone, upcoming: readonly number[]): Zone {
  let { lo, hi } = anchor;
  for (const f of upcoming) {
    if (f <= 0) continue;
    lo = Math.min(lo, f);
    hi = Math.max(hi, f);
  }
  return { lo: clamp(lo, 1, FRETS), hi: clamp(hi, 1, FRETS) };
}

/**
 * Camera target for a zone: lateral position and zoom factor (1 frames a four-fret anchor on a
 * 16:10 window; wider zones and narrower windows pull the camera back).
 */
export function framing(zone: Zone, aspect: number): { x: number; scale: number } {
  const width = zone.hi - zone.lo + 1;
  const portrait = Math.max(1, 1.6 / Math.max(aspect, 0.3));
  return {
    x: (zoneLeft(zone) + zoneRight(zone)) / 2,
    scale: clamp((width + 4) / 8, 0.9, 2.2) * portrait,
  };
}

/** Exponential approach of `target` with time constant `tau`, independent of the frame rate. */
export function approach(current: number, target: number, dt: number, tau: number): number {
  return target + (current - target) * Math.exp(-Math.max(dt, 0) / tau);
}

/**
 * Order in which notes reach the strike line, 0 for the next one to play, fed the notes in time
 * order once per frame; notes struck together (a chord, a double stop) share their rank.
 */
export class Arrivals {
  private rank = -1;
  private last = -Infinity;

  constructor(private readonly together = 0.03) {}

  reset(): void {
    this.rank = -1;
    this.last = -Infinity;
  }

  /** Rank of a note at `time`, `ahead` seconds away; null once played or gone past. */
  next(time: number, pending: boolean, ahead: number): number | null {
    if (!pending || ahead < -0.05) return null;
    if (time > this.last + this.together) {
      this.rank++;
      this.last = time;
    }
    return this.rank;
  }
}

/** Where a note heard out of a song was most likely played, in `tuning` (open strings, low
 * first): the string giving the lowest fret, open strings first; the low string below it. */
export function placeNote(midi: number, tuning: readonly number[]): { string: number; fret: number } {
  let best = { string: 0, fret: Math.max(0, midi - tuning[0]) };
  tuning.forEach((open, s) => {
    const fret = midi - open;
    if (fret >= 0 && fret < best.fret) best = { string: s, fret };
  });
  return { string: best.string, fret: Math.min(FRETS, best.fret) };
}
