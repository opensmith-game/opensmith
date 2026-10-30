import { describe, expect, it } from 'vitest';
import type { SongEntry } from '../src/engine/types';
import { EVERY_SONG, fold, phraseMastery, selectSongs, sourceOf, tuningsOf } from '../src/library';
import { emptyProgress, type Progress } from '../src/progress';

const song = (id: string, title: string, artist: string, extra: Partial<SongEntry> = {}): SongEntry => ({
  id,
  title,
  artist,
  duration: 200,
  tuning: 'E Standard',
  arrangement: 'Lead',
  notes: 100,
  chords: 10,
  ...extra,
});

const SONGS = [
  song('a', 'Back Alley', 'Vautours', { tuning: 'Drop C', source: 'audio' }),
  song('b', 'Come Back Home', 'Larsen', { tuning: 'D Standard', source: 'guitarpro', hasAudio: true }),
  song('c', 'Électrique', 'Bande Test', { source: 'some-plugin', album: 'Démos' }),
  song('d', 'Tab Only', 'Someone', { source: 'guitarpro', hasAudio: false }),
];

const PROGRESS: Progress = {
  a: { ...emptyProgress(), plays: 3, mastery: 0.2, lastPlayed: '2026-10-01T10:00:00Z' },
  c: { ...emptyProgress(), plays: 1, mastery: 0.8, lastPlayed: '2026-10-20T10:00:00Z', mastered: { V: 4 } },
};

describe('library', () => {
  it('tells where the notes come from', () => {
    expect(SONGS.map(sourceOf)).toEqual(['ai', 'tab', 'chart', 'sheet']);
  });

  it('finds songs by any word, accents or not', () => {
    const find = (text: string) => selectSongs(SONGS, PROGRESS, { ...EVERY_SONG, text }).map((s) => s.id);
    expect(fold('Électrique')).toBe('electrique');
    expect(find('electrique')).toEqual(['c']);
    expect(find('larsen come')).toEqual(['b']);
    expect(find('demos')).toEqual(['c']);
    expect(find('zz')).toEqual([]);
  });

  it('sorts by last played, title, artist or mastery', () => {
    const order = (sort: (typeof EVERY_SONG)['sort']) => selectSongs(SONGS, PROGRESS, { ...EVERY_SONG, sort }).map((s) => s.id);
    expect(order('recent')).toEqual(['c', 'a', 'b', 'd']);
    expect(order('title')).toEqual(['a', 'b', 'c', 'd']);
    expect(order('artist')).toEqual(['c', 'b', 'd', 'a']);
    expect(order('mastery')).toEqual(['c', 'a', 'b', 'd']);
  });

  it('filters by source and tuning', () => {
    expect(selectSongs(SONGS, PROGRESS, { ...EVERY_SONG, source: 'tab' }).map((s) => s.id)).toEqual(['b']);
    expect(selectSongs(SONGS, PROGRESS, { ...EVERY_SONG, tuning: 'Drop C' }).map((s) => s.id)).toEqual(['a']);
    expect(tuningsOf(SONGS)).toEqual(['E Standard', 'D Standard', 'Drop C']);
  });

  it('shows each phrase with the level mastered', () => {
    const s = song('c', 'X', 'Y', { phrases: ['V', 'C', 'Z'], phraseSections: ['Verse 1', 'chorus', 'Section 3'], maxDifficulty: 4 });
    expect(phraseMastery(s, PROGRESS.c)).toEqual([
      { name: 'V', label: 'Couplet 1', level: 4, levels: 5 },
      { name: 'C', label: 'Refrain', level: -1, levels: 5 },
      { name: 'Z', label: 'Phrase Z', level: -1, levels: 5 },
    ]);
  });
});
