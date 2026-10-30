import { describe, expect, it } from 'vitest';
import { initials } from '../src/cover';

describe('song pictures', () => {
  it('fall back to the initials of the title', () => {
    expect(initials('Demo Rock')).toBe('DR');
    expect(initials('Come Back Home')).toBe('CB');
    expect(initials('Anthem')).toBe('A');
    expect(initials('77 Reasons')).toBe('7R');
    expect(initials('— (live) —')).toBe('L');
    expect(initials('')).toBe('♪');
  });
});
