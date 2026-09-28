import { describe, expect, test } from 'bun:test';
import { displaySpeaker, isCombinedSpeaker, isUserSpeaker, speakerKey } from '../../src/lib/speaker-labels';

describe('isUserSpeaker', () => {
  test('matches only an exact "You" label, case-insensitively and trimmed', () => {
    expect(isUserSpeaker('You')).toBe(true);
    expect(isUserSpeaker('you')).toBe(true);
    expect(isUserSpeaker('  You  ')).toBe(true);
  });

  test('matches a single name ending in "(You)"', () => {
    expect(isUserSpeaker('Felipe (You)')).toBe(true);
    expect(isUserSpeaker('Felipe ( you )')).toBe(true);
  });

  test('does not match a combined label even when "You" is one of its parts', () => {
    expect(isUserSpeaker('You + Speaker 1')).toBe(false);
    expect(isUserSpeaker('Speaker 1 + You')).toBe(false);
    expect(isUserSpeaker('Felipe (You) + Speaker 2')).toBe(false);
  });

  test('does not match labels that merely start with "you" as a prefix', () => {
    expect(isUserSpeaker('Youssef')).toBe(false);
    expect(isUserSpeaker('Younes')).toBe(false);
  });

  test('handles missing/empty input', () => {
    expect(isUserSpeaker(undefined)).toBe(false);
    expect(isUserSpeaker('')).toBe(false);
    expect(isUserSpeaker('   ')).toBe(false);
  });
});

describe('isCombinedSpeaker', () => {
  test('detects the " + " separator used by offline diarization', () => {
    expect(isCombinedSpeaker('You + Speaker 1')).toBe(true);
    expect(isCombinedSpeaker('Speaker 1 + Speaker 2 + Guest')).toBe(true);
    expect(isCombinedSpeaker('Speaker 1')).toBe(false);
    expect(isCombinedSpeaker(undefined)).toBe(false);
  });
});

describe('displaySpeaker', () => {
  test('substitutes the user display name for an exact "You" label', () => {
    expect(displaySpeaker('You', 'Felipe')).toBe('Felipe (You)');
    expect(displaySpeaker('You', '')).toBe('You');
  });

  test('leaves a plain custom or generated label untouched', () => {
    expect(displaySpeaker('Speaker 1', 'Felipe')).toBe('Speaker 1');
    expect(displaySpeaker('Johonnatan', 'Felipe')).toBe('Johonnatan');
  });

  test('renders a combined label part by part, mapping only the "You" part to the display name', () => {
    expect(displaySpeaker('You + Speaker 1', 'Felipe')).toBe('Felipe + Speaker 1');
    expect(displaySpeaker('Speaker 1 + You', '')).toBe('Speaker 1 + You');
    expect(displaySpeaker('Speaker 1 + Speaker 2', 'Felipe')).toBe('Speaker 1 + Speaker 2');
  });
});

describe('speakerKey', () => {
  test('groups every "You" spelling under one key', () => {
    expect(speakerKey('You')).toBe(speakerKey('you'));
    expect(speakerKey('Felipe (You)')).toBe('__you__');
  });

  test('gives a combined label its own key, distinct from either of its parts', () => {
    expect(speakerKey('You + Speaker 1')).not.toBe('__you__');
    expect(speakerKey('You + Speaker 1')).not.toBe(speakerKey('Speaker 1'));
    expect(speakerKey('You + Speaker 1')).toBe(speakerKey('you + speaker 1'));
  });

  test('falls back to a stable key for missing labels', () => {
    expect(speakerKey(undefined)).toBe('__unknown__');
    expect(speakerKey('')).toBe('__unknown__');
  });
});
