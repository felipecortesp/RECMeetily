import { describe, expect, test } from 'bun:test';
import { formatParticipantDuration, summarizeParticipants } from '../../src/lib/participants';

const seg = (id: string, start: number, end: number | undefined, speaker?: string | null) => ({
  id,
  timestamp: start,
  endTime: end,
  speaker,
});

describe('summarizeParticipants', () => {
  test('counts segments and sums duration per speaker', () => {
    const result = summarizeParticipants([
      seg('a', 0, 10, 'Speaker 1'),
      seg('b', 10, 15, 'Speaker 2'),
      seg('c', 20, 30, 'Speaker 1'),
    ]);
    const s1 = result.find((p) => p.label === 'Speaker 1')!;
    expect(s1.segmentCount).toBe(2);
    expect(s1.totalSeconds).toBe(20);
    expect(s1.firstSegmentId).toBe('a');
    expect(s1.firstTimestamp).toBe(0);
    expect(result.find((p) => p.label === 'Speaker 2')!.totalSeconds).toBe(5);
  });

  test('puts You first, then sorts by total time descending', () => {
    const result = summarizeParticipants([
      seg('a', 0, 100, 'Speaker 1'),
      seg('b', 100, 110, 'You'),
      seg('c', 110, 150, 'Speaker 2'),
    ]);
    expect(result.map((p) => p.label)).toEqual(['You', 'Speaker 1', 'Speaker 2']);
  });

  test('combined labels count toward each part without creating their own entry', () => {
    const result = summarizeParticipants([
      seg('a', 0, 10, 'You + Speaker 1'),
      seg('b', 10, 14, 'Speaker 1'),
    ]);
    expect(result.map((p) => p.label).sort()).toEqual(['Speaker 1', 'You']);
    const you = result.find((p) => p.label === 'You')!;
    expect(you.segmentCount).toBe(1);
    expect(you.totalSeconds).toBe(10);
    const s1 = result.find((p) => p.label === 'Speaker 1')!;
    expect(s1.segmentCount).toBe(2);
    expect(s1.totalSeconds).toBe(14);
    expect(s1.firstSegmentId).toBe('a');
  });

  test('ignores null, undefined and blank speakers', () => {
    const result = summarizeParticipants([
      seg('a', 0, 5, null),
      seg('b', 5, 6, undefined),
      seg('c', 6, 7, '   '),
      seg('d', 7, 9, 'Dima'),
    ]);
    expect(result).toHaveLength(1);
    expect(result[0].label).toBe('Dima');
  });

  test('treats missing or inverted end times as zero duration', () => {
    const result = summarizeParticipants([seg('a', 10, undefined, 'Dima'), seg('b', 20, 15, 'Dima')]);
    expect(result[0].segmentCount).toBe(2);
    expect(result[0].totalSeconds).toBe(0);
  });

  test('returns an empty list for no segments', () => {
    expect(summarizeParticipants([])).toEqual([]);
  });
});

describe('formatParticipantDuration', () => {
  test('formats seconds, minutes and hours', () => {
    expect(formatParticipantDuration(45)).toBe('45s');
    expect(formatParticipantDuration(185)).toBe('3m 05s');
    expect(formatParticipantDuration(3720)).toBe('1h 02m');
  });
});
