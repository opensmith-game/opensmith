import { describe, expect, it } from 'vitest';
import { partName, phraseLabels, sectionLabels, songParts } from '../src/sections';

const span = (name: string, start: number, end: number) => ({ name, start, end });
const phrase = (name: string, start: number, end: number) => ({ name, start, end, maxDifficulty: 4 });

describe('sections', () => {
  it('names the parts in French, as tabs and charts write them', () => {
    expect(partName('Verse 1')).toEqual({ name: 'Couplet', number: '1' });
    expect(partName('Pre Chorus 2')).toEqual({ name: 'Pré-refrain', number: '2' });
    expect(partName('Guitar Solo')).toEqual({ name: 'Solo', number: '' });
    expect(partName('noguitar')).toEqual({ name: 'Sans guitare', number: '' });
    expect(partName('riff a')).toEqual({ name: 'Riff a', number: '' });
    expect(partName('Section 3')).toBeNull();
    expect(partName('  ')).toBeNull();
  });

  it('numbers a part that comes back without a number', () => {
    const names = ['intro', 'Break', 'chorus', 'Break', 'Interlude', 'Interlude 2', 'Section 9'];
    expect(sectionLabels(names.map((n) => ({ name: n })))).toEqual([
      'Intro',
      'Break 1',
      'Refrain',
      'Break 2',
      'Interlude',
      'Interlude 2',
      null,
    ]);
  });

  it('calls each phrase by its part, with its place when the part has several', () => {
    const chart = {
      sections: [span('Intro', 0, 10), span('Verse 1', 10, 30), span('Chorus 1', 30, 40)],
      phrases: [
        phrase('rest', 0, 4),
        phrase('A', 4, 10),
        phrase('B', 10, 20),
        phrase('rest', 20, 25),
        phrase('C', 25, 30),
        phrase('D', 30, 40),
      ],
    };
    expect(songParts(chart)).toEqual([
      { label: 'Intro', first: 0, last: 1 },
      { label: 'Couplet 1', first: 2, last: 4 },
      { label: 'Refrain 1', first: 5, last: 5 },
    ]);
    expect(phraseLabels(chart)).toEqual(['', 'Intro', 'Couplet 1 · 1/2', '', 'Couplet 1 · 2/2', 'Refrain 1']);
  });

  it('keeps the letters when the sections have no names', () => {
    const chart = { sections: [span('Section 1', 0, 20)], phrases: [phrase('A', 0, 10), phrase('B', 10, 20)] };
    expect(songParts(chart)).toBeNull();
    expect(phraseLabels(chart)).toEqual(['Phrase A', 'Phrase B']);
  });
});
