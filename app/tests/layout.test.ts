import { describe, expect, it } from 'vitest';
import { Arrivals, STRING_BASE, anchorZone, approach, focusZone, framing, laneCenter, placeNote, stringY, timeZ } from '../src/layout';

describe('highway layout', () => {
  it('puts the low E string on top by default, like the guitar seen by its player', () => {
    const ys = [0, 1, 2, 3, 4, 5].map((s) => stringY(s, false));
    expect(ys).toEqual([...ys].sort((a, b) => b - a));
    expect(stringY(5, false)).toBeCloseTo(STRING_BASE);
  });

  it('inverts to tablature order', () => {
    const ys = [0, 1, 2, 3, 4, 5].map((s) => stringY(s, true));
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
    expect(stringY(0, true)).toBeCloseTo(STRING_BASE);
  });

  it('centres notes between fret wires', () => {
    expect(laneCenter(1)).toBeCloseTo(0.5);
    expect(laneCenter(12)).toBeCloseTo(11.5);
  });

  it('brings notes toward the player as the song plays', () => {
    expect(timeZ(5, 4, 10)).toBeCloseTo(-10);
    expect(timeZ(4, 4, 10)).toBeCloseTo(0);
    expect(timeZ(3.9, 4, 10)).toBeGreaterThan(0);
  });

  it('frames the anchor and widens for notes outside it', () => {
    const zone = anchorZone({ time: 0, fret: 5, width: 4 });
    expect(zone).toEqual({ lo: 5, hi: 8 });
    expect(focusZone(zone, [0, 3, 6, 10])).toEqual({ lo: 3, hi: 10 });
    const narrow = framing(zone, 1.6);
    const wide = framing({ lo: 3, hi: 10 }, 1.6);
    expect(narrow.x).toBeCloseTo(6);
    expect(wide.scale).toBeGreaterThan(narrow.scale);
    expect(framing(zone, 0.8).scale).toBeGreaterThan(narrow.scale);
  });

  it('keeps anchors on the neck', () => {
    expect(anchorZone({ time: 0, fret: 22, width: 5 })).toEqual({ lo: 22, hi: 24 });
    expect(anchorZone({ time: 0, fret: 0, width: 4 })).toEqual({ lo: 1, hi: 3 });
  });

  it('moves the camera the same way at 60 and 120 frames per second', () => {
    let a = 0;
    for (let i = 0; i < 60; i++) a = approach(a, 10, 1 / 60, 0.3);
    let b = 0;
    for (let i = 0; i < 120; i++) b = approach(b, 10, 1 / 120, 0.3);
    expect(a).toBeCloseTo(b, 6);
    expect(a).toBeGreaterThan(9.5);
  });

  it('ranks the notes to come, the notes of a chord together, played ones aside', () => {
    const arrivals = new Arrivals();
    const now = 10;
    const notes = [
      { time: 9.9, pending: true }, // just gone past the line
      { time: 10.5, pending: false }, // already played
      { time: 11, pending: true },
      { time: 11.01, pending: true }, // same strum
      { time: 12, pending: true },
      { time: 13, pending: true },
    ];
    expect(notes.map((n) => arrivals.next(n.time, n.pending, n.time - now))).toEqual([null, null, 0, 0, 1, 2]);
    arrivals.reset();
    expect(arrivals.next(12, true, 2)).toBe(0);
  });

  it('places a note heard out of a song on the string nearest the nut', () => {
    const standard = [40, 45, 50, 55, 59, 64];
    expect(placeNote(40, standard)).toEqual({ string: 0, fret: 0 }); // low E open
    expect(placeNote(45, standard)).toEqual({ string: 1, fret: 0 }); // A open, not the low E at 5
    expect(placeNote(52, standard)).toEqual({ string: 2, fret: 2 });
    expect(placeNote(76, standard)).toEqual({ string: 5, fret: 12 });
    expect(placeNote(36, [36, 43, 48, 53, 57, 62])).toEqual({ string: 0, fret: 0 }); // Drop C's low C
    expect(placeNote(35, standard)).toEqual({ string: 0, fret: 0 }); // below the guitar: the low string
  });
});
