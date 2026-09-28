//! Splits transcript rows at speaker changes using word timings and
//! diarization turns. Pure logic: no I/O, no model calls.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::audio::word_timing::WordTiming;
use crate::diarization::DiarizationSegment;

/// Speaker turns shorter than this are absorbed into a neighbour so short
/// back-channels ("uhum") or clustering flicker don't chop sentences apart.
pub const MIN_SPEAKER_TURN_SECS: f64 = 0.5;
/// Two speakers inside a piece only count as simultaneous speech when their
/// diarization segments overlap for at least this long in total.
pub const MIN_SIMULTANEOUS_SECS: f64 = 0.5;
/// Combined labels never list more than this many speakers.
pub const MAX_COMBINED_SPEAKERS: usize = 3;

/// Half a Parakeet frame; gives zero-length words a window to be assigned by.
const ZERO_LEN_HALF_WINDOW: f64 = 0.04;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// Indices into the row's words (half-open).
    pub words: Range<usize>,
    pub start: f64,
    pub end: f64,
    /// Primary speaker first; more only for real simultaneous speech. Empty when
    /// no turn overlaps the piece at all.
    pub speakers: Vec<usize>,
}

pub fn turns_from_segments(segments: &[DiarizationSegment]) -> Vec<Turn> {
    let mut turns: Vec<Turn> = segments
        .iter()
        .map(|s| Turn {
            start: s.start as f64,
            end: s.end as f64,
            speaker: s.speaker,
        })
        .filter(|t| t.end > t.start)
        .collect();
    turns.sort_by(|a, b| a.start.total_cmp(&b.start));
    turns
}

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Speaker with the largest total overlap with [start, end]; ties go to the
/// lower id so results are deterministic.
fn primary_speaker(start: f64, end: f64, turns: &[Turn]) -> Option<usize> {
    let mut totals: BTreeMap<usize, f64> = BTreeMap::new();
    for t in turns {
        let o = overlap(t.start, t.end, start, end);
        if o > 0.0 {
            *totals.entry(t.speaker).or_default() += o;
        }
    }
    let mut best: Option<(usize, f64)> = None;
    for (spk, total) in totals {
        if best.map_or(true, |(_, b)| total > b) {
            best = Some((spk, total));
        }
    }
    best.map(|(s, _)| s)
}

/// Primary speaker first, then others whose speech truly overlaps the primary's
/// inside [start, end], strongest first, capped at `MAX_COMBINED_SPEAKERS`.
fn with_simultaneous(primary: usize, start: f64, end: f64, turns: &[Turn]) -> Vec<usize> {
    let mut simul: BTreeMap<usize, f64> = BTreeMap::new();
    for p in turns.iter().filter(|t| t.speaker == primary) {
        for o in turns.iter().filter(|t| t.speaker != primary) {
            let len = overlap(
                p.start.max(o.start),
                p.end.min(o.end),
                start,
                end,
            );
            if len > 0.0 {
                *simul.entry(o.speaker).or_default() += len;
            }
        }
    }
    let mut others: Vec<(usize, f64)> = simul
        .into_iter()
        .filter(|&(_, t)| t >= MIN_SIMULTANEOUS_SECS)
        .collect();
    others.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut speakers = vec![primary];
    speakers.extend(others.into_iter().map(|(s, _)| s));
    speakers.truncate(MAX_COMBINED_SPEAKERS);
    speakers
}

/// Speakers of a whole interval without word timings: primary by overlap, plus
/// others only for real simultaneous speech (same rule as pieces).
pub fn speakers_for_interval(start: f64, end: f64, turns: &[Turn]) -> Vec<usize> {
    match primary_speaker(start, end, turns) {
        Some(p) => with_simultaneous(p, start, end, turns),
        None => Vec::new(),
    }
}

#[derive(Debug, Clone)]
struct Run {
    words: Range<usize>,
    speaker: usize,
}

fn run_bounds(run: &Run, words: &[WordTiming]) -> (f64, f64) {
    (words[run.words.start].start, words[run.words.end - 1].end)
}

fn run_duration(run: &Run, words: &[WordTiming]) -> f64 {
    let (s, e) = run_bounds(run, words);
    e - s
}

fn merge_adjacent_same_speaker(runs: Vec<Run>) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::with_capacity(runs.len());
    for r in runs {
        match out.last_mut() {
            Some(last) if last.speaker == r.speaker => last.words.end = r.words.end,
            _ => out.push(r),
        }
    }
    out
}

/// Splits one transcript row at speaker changes.
pub fn split_row(row_start: f64, row_end: f64, words: &[WordTiming], turns: &[Turn]) -> Vec<Piece> {
    if words.is_empty() {
        return vec![Piece {
            words: 0..0,
            start: row_start,
            end: row_end,
            speakers: speakers_for_interval(row_start, row_end, turns),
        }];
    }

    let assigned: Vec<Option<usize>> = words
        .iter()
        .map(|w| {
            let (a, b) = if w.end > w.start {
                (w.start, w.end)
            } else {
                (w.start - ZERO_LEN_HALF_WINDOW, w.start + ZERO_LEN_HALF_WINDOW)
            };
            primary_speaker(a, b, turns)
        })
        .collect();

    let Some(first_assigned) = assigned.iter().flatten().next().copied() else {
        return vec![Piece {
            words: 0..words.len(),
            start: row_start,
            end: row_end,
            speakers: Vec::new(),
        }];
    };

    // Gaps inherit the previous word's speaker; leading gaps the first known one.
    let mut prev = first_assigned;
    let speaker_of: Vec<usize> = assigned
        .iter()
        .map(|a| {
            prev = a.unwrap_or(prev);
            prev
        })
        .collect();

    let mut runs: Vec<Run> = Vec::new();
    for (i, &spk) in speaker_of.iter().enumerate() {
        match runs.last_mut() {
            Some(last) if last.speaker == spk => last.words.end = i + 1,
            _ => runs.push(Run {
                words: i..i + 1,
                speaker: spk,
            }),
        }
    }

    while runs.len() > 1 {
        let mut shortest: Option<(usize, f64)> = None;
        for (i, r) in runs.iter().enumerate() {
            let d = run_duration(r, words);
            if d < MIN_SPEAKER_TURN_SECS && shortest.map_or(true, |(_, best)| d < best) {
                shortest = Some((i, d));
            }
        }
        let Some((i, _)) = shortest else { break };

        let left = i.checked_sub(1);
        let right = (i + 1 < runs.len()).then_some(i + 1);
        match (left, right) {
            (Some(l), Some(r)) if runs[l].speaker == runs[r].speaker => {
                runs[l].words.end = runs[r].words.end;
                runs.drain(i..=r);
            }
            (Some(l), Some(r)) => {
                let target = if run_duration(&runs[r], words) > run_duration(&runs[l], words) {
                    r
                } else {
                    l
                };
                absorb_into(&mut runs, i, target);
            }
            (Some(l), None) => absorb_into(&mut runs, i, l),
            (None, Some(r)) => absorb_into(&mut runs, i, r),
            (None, None) => break,
        }
        runs = merge_adjacent_same_speaker(runs);
    }

    let last_idx = runs.len() - 1;
    runs.iter()
        .enumerate()
        .map(|(i, run)| {
            let (mut start, mut end) = run_bounds(run, words);
            if i == 0 {
                start = row_start;
            }
            if i == last_idx {
                end = row_end;
            }
            let end = end.max(start);
            Piece {
                words: run.words.clone(),
                start,
                end,
                speakers: with_simultaneous(run.speaker, start, end, turns),
            }
        })
        .collect()
}

/// Merges `runs[from]` into the adjacent run `to`, which keeps its speaker.
fn absorb_into(runs: &mut Vec<Run>, from: usize, to: usize) {
    let removed = runs.remove(from);
    let to = if to > from { to - 1 } else { to };
    let t = &mut runs[to];
    t.words.start = t.words.start.min(removed.words.start);
    t.words.end = t.words.end.max(removed.words.end);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(spec: &[(&str, f64, f64)]) -> Vec<WordTiming> {
        spec.iter()
            .map(|&(text, start, end)| WordTiming {
                text: text.to_string(),
                start,
                end,
            })
            .collect()
    }

    fn turns(spec: &[(f64, f64, usize)]) -> Vec<Turn> {
        spec.iter()
            .map(|&(start, end, speaker)| Turn { start, end, speaker })
            .collect()
    }

    /// One word per second starting at 0.
    fn secs_words(n: usize) -> Vec<WordTiming> {
        (0..n)
            .map(|i| WordTiming {
                text: format!("w{i}"),
                start: i as f64,
                end: i as f64 + 1.0,
            })
            .collect()
    }

    fn assert_tiles(pieces: &[Piece], n_words: usize) {
        assert_eq!(pieces[0].words.start, 0);
        assert_eq!(pieces.last().unwrap().words.end, n_words);
        for pair in pieces.windows(2) {
            assert_eq!(pair[0].words.end, pair[1].words.start);
        }
    }

    #[test]
    fn single_speaker_row_is_one_piece() {
        let w = secs_words(4);
        let t = turns(&[(0.0, 4.0, 0)]);
        let p = split_row(0.0, 4.2, &w, &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].words, 0..4);
        assert_eq!((p[0].start, p[0].end), (0.0, 4.2));
        assert_eq!(p[0].speakers, vec![0]);
    }

    #[test]
    fn speaker_change_mid_row_splits_at_first_word_of_new_speaker() {
        let w = secs_words(6);
        let t = turns(&[(0.0, 3.0, 0), (3.0, 6.0, 1)]);
        let p = split_row(-0.1, 6.3, &w, &t);
        assert_eq!(p.len(), 2);
        assert_tiles(&p, 6);
        assert_eq!(p[0].words, 0..3);
        assert_eq!(p[1].words, 3..6);
        assert_eq!(p[0].start, -0.1);
        assert_eq!(p[0].end, 3.0);
        assert_eq!(p[1].start, 3.0);
        assert_eq!(p[1].end, 6.3);
        assert_eq!(p[0].speakers, vec![0]);
        assert_eq!(p[1].speakers, vec![1]);
    }

    #[test]
    fn short_flip_in_the_middle_is_absorbed() {
        let w = words(&[
            ("a", 0.0, 1.0),
            ("b", 1.0, 2.0),
            ("uhum", 2.0, 2.3),
            ("c", 2.3, 3.3),
            ("d", 3.3, 4.3),
        ]);
        let t = turns(&[(0.0, 2.0, 0), (2.0, 2.3, 1), (2.3, 4.3, 0)]);
        let p = split_row(0.0, 4.3, &w, &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].words, 0..5);
        assert_eq!(p[0].speakers, vec![0]);
    }

    #[test]
    fn short_run_at_the_edge_is_absorbed_into_previous() {
        let w = words(&[("a", 0.0, 1.0), ("b", 1.0, 2.0), ("ok", 2.0, 2.3)]);
        let t = turns(&[(0.0, 2.0, 0), (2.0, 2.3, 1)]);
        let p = split_row(0.0, 2.3, &w, &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].words, 0..3);
        assert_eq!(p[0].speakers, vec![0]);
    }

    #[test]
    fn short_run_at_the_start_is_absorbed_into_next() {
        let w = words(&[("ok", 0.0, 0.3), ("a", 0.3, 1.3), ("b", 1.3, 2.3)]);
        let t = turns(&[(0.0, 0.3, 1), (0.3, 2.3, 0)]);
        let p = split_row(0.0, 2.3, &w, &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].speakers, vec![0]);
    }

    #[test]
    fn a_b_a_with_long_runs_gives_three_pieces() {
        let w = secs_words(6);
        let t = turns(&[(0.0, 2.0, 0), (2.0, 4.0, 1), (4.0, 6.0, 0)]);
        let p = split_row(0.0, 6.0, &w, &t);
        assert_eq!(p.len(), 3);
        assert_tiles(&p, 6);
        let spk: Vec<_> = p.iter().map(|x| x.speakers.clone()).collect();
        assert_eq!(spk, vec![vec![0], vec![1], vec![0]]);
    }

    #[test]
    fn real_overlap_lists_both_speakers() {
        let w = words(&[("a", 0.0, 2.0), ("b", 2.0, 4.0), ("c", 4.0, 6.0)]);
        let t = turns(&[(0.0, 6.0, 0), (2.0, 3.2, 1)]);
        let p = split_row(0.0, 6.0, &w, &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].speakers, vec![0, 1]);
    }

    #[test]
    fn brief_overlap_lists_only_primary() {
        let w = words(&[("a", 0.0, 2.0), ("b", 2.0, 4.0), ("c", 4.0, 6.0)]);
        let t = turns(&[(0.0, 6.0, 0), (2.0, 2.3, 1)]);
        let p = split_row(0.0, 6.0, &w, &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].speakers, vec![0]);
    }

    #[test]
    fn no_turns_gives_one_piece_without_speakers() {
        let w = secs_words(3);
        let p = split_row(0.0, 3.0, &w, &[]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].words, 0..3);
        assert_eq!((p[0].start, p[0].end), (0.0, 3.0));
        assert!(p[0].speakers.is_empty());
    }

    #[test]
    fn uncovered_words_take_the_neighbour_speaker() {
        // Words 2 and 3 fall in a gap between turns; leading word 0 too.
        let w = secs_words(6);
        let t = turns(&[(1.0, 2.0, 0), (4.0, 6.0, 1)]);
        let p = split_row(0.0, 6.0, &w, &t);
        assert_tiles(&p, 6);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].words, 0..4);
        assert_eq!(p[0].speakers, vec![0]);
        assert_eq!(p[1].words, 4..6);
        assert_eq!(p[1].speakers, vec![1]);
    }

    #[test]
    fn zero_length_word_is_still_assigned() {
        let w = words(&[("a", 0.0, 1.0), ("b", 1.0, 2.0), ("x", 3.0, 3.0), ("y", 3.0, 4.0)]);
        let t = turns(&[(0.0, 2.0, 0), (2.9, 4.0, 1)]);
        let p = split_row(0.0, 4.0, &w, &t);
        assert_eq!(p.len(), 2);
        assert_eq!(p[1].words, 2..4);
    }

    #[test]
    fn empty_words_use_interval_speakers() {
        let t = turns(&[(0.0, 3.0, 2), (1.0, 2.0, 5)]);
        let p = split_row(0.0, 3.0, &[], &t);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].words, 0..0);
        assert_eq!((p[0].start, p[0].end), (0.0, 3.0));
        assert_eq!(p[0].speakers, vec![2, 5]);
    }

    #[test]
    fn ties_go_to_the_lower_speaker_id() {
        let w = words(&[("a", 0.0, 2.0)]);
        let t = turns(&[(0.0, 1.0, 7), (1.0, 2.0, 3)]);
        let p = split_row(0.0, 2.0, &w, &t);
        assert_eq!(p[0].speakers[0], 3);
        assert_eq!(speakers_for_interval(0.0, 2.0, &t)[0], 3);
    }

    #[test]
    fn interval_primary_plus_simultaneous() {
        let t = turns(&[(0.0, 4.0, 0), (1.0, 2.0, 1), (3.0, 3.2, 2)]);
        assert_eq!(speakers_for_interval(0.0, 4.0, &t), vec![0, 1]);
    }

    #[test]
    fn interval_without_overlap_is_empty() {
        let t = turns(&[(0.0, 1.0, 0)]);
        assert!(speakers_for_interval(2.0, 3.0, &t).is_empty());
        assert!(speakers_for_interval(0.0, 1.0, &[]).is_empty());
    }

    #[test]
    fn turns_from_segments_sorts_and_drops_empty() {
        let seg = |start: f32, end: f32, speaker: usize| DiarizationSegment {
            start,
            end,
            speaker,
            overlapped: false,
        };
        let out = turns_from_segments(&[seg(5.0, 6.0, 1), seg(2.0, 2.0, 0), seg(1.0, 3.0, 2)]);
        assert_eq!(
            out,
            vec![
                Turn { start: 1.0, end: 3.0, speaker: 2 },
                Turn { start: 5.0, end: 6.0, speaker: 1 },
            ]
        );
    }

    #[test]
    fn combined_speakers_are_capped() {
        let t = turns(&[(0.0, 4.0, 0), (0.0, 4.0, 1), (0.0, 3.0, 2), (0.0, 2.0, 3)]);
        let s = speakers_for_interval(0.0, 4.0, &t);
        assert_eq!(s.len(), MAX_COMBINED_SPEAKERS);
        assert_eq!(s, vec![0, 1, 2]);
    }
}
