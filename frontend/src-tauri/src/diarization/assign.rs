//! Plans and persists per-row speaker labels (and row splits) from an offline
//! diarization result. Planning is pure; only `apply_plans` touches the DB.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use sqlx::SqliteConnection;
use uuid::Uuid;

use crate::audio::word_timing::WordTiming;
use crate::diarization::split::{
    speakers_for_interval, split_row, Piece, Turn, MAX_COMBINED_SPEAKERS,
};

#[derive(Debug, Clone)]
pub struct RowInput {
    pub id: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub speaker: Option<String>,
    pub words: Option<Vec<WordTiming>>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPiece {
    pub start: f64,
    pub end: f64,
    pub text: Option<String>,
    pub words: Option<Vec<WordTiming>>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RowPlan {
    /// Row must not be modified; `label` is its current speaker, if any.
    Untouched { id: String, label: Option<String> },
    /// Only the speaker changes; text and words stay as they are.
    Speaker { id: String, label: Option<String> },
    /// Row is cut into several pieces; the first reuses the row's id.
    Split {
        id: String,
        source: Option<String>,
        pieces: Vec<PlannedPiece>,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Track {
    Mic,
    System,
    Any,
}

fn is_generated_part(part: &str) -> bool {
    part.eq_ignore_ascii_case("guest")
        || part.eq_ignore_ascii_case("you")
        || part.to_ascii_lowercase().starts_with("speaker ")
}

/// A label the user (or a rename) chose, as opposed to a generated one.
pub(super) fn is_named(label: &str) -> bool {
    let t = label.trim();
    !t.is_empty() && !t.split(" + ").all(is_generated_part)
}

fn row_track(row: &RowInput, used_source_tracks: bool) -> Track {
    match row.source.as_deref() {
        Some("mic") => return Track::Mic,
        Some("system") => return Track::System,
        _ => {}
    }
    match row.speaker.as_deref() {
        Some(s) if used_source_tracks && s.eq_ignore_ascii_case("you") => Track::Mic,
        Some(s) if used_source_tracks && s.eq_ignore_ascii_case("guest") => Track::System,
        _ => Track::Any,
    }
}

fn valid_times(row: &RowInput) -> Option<(f64, f64)> {
    match (row.start, row.end) {
        (Some(s), Some(e)) if s.is_finite() && e.is_finite() && e > s => Some((s, e)),
        _ => None,
    }
}

fn row_turns(
    track: Track,
    start: f64,
    end: f64,
    turns: &[Turn],
    used_source_tracks: bool,
) -> Vec<Turn> {
    turns
        .iter()
        .filter(|t| match (used_source_tracks, track) {
            (true, Track::Mic) => t.speaker == 0,
            (true, Track::System) => t.speaker != 0,
            _ => true,
        })
        .filter(|t| t.end > start && t.start < end)
        .copied()
        .collect()
}

pub fn plan_rows(
    rows: Vec<RowInput>,
    turns: &[Turn],
    user_speaker: Option<usize>,
    used_source_tracks: bool,
    allow_remote_source_hint: bool,
) -> Vec<RowPlan> {
    struct Prepared {
        row: RowInput,
        track: Track,
        times: Option<(f64, f64)>,
        turns: Vec<Turn>,
        /// Existing custom name when it is a single name (no " + ").
        own_name: Option<String>,
        dominant: Option<usize>,
    }

    let prepared: Vec<Prepared> = rows
        .into_iter()
        .map(|row| {
            let track = row_track(&row, used_source_tracks);
            let times = valid_times(&row);
            let turns = times
                .map(|(s, e)| row_turns(track, s, e, turns, used_source_tracks))
                .unwrap_or_default();
            let own_name = row
                .speaker
                .as_deref()
                .map(str::trim)
                .filter(|l| is_named(l) && !l.contains(" + "))
                .map(str::to_string);
            let dominant = match (&own_name, times) {
                (Some(_), Some((s, e))) => speakers_for_interval(s, e, &turns).first().copied(),
                _ => None,
            };
            Prepared { row, track, times, turns, own_name, dominant }
        })
        .collect();

    // Each cluster inherits the custom name that covers most of its seconds.
    let mut votes: HashMap<usize, BTreeMap<String, f64>> = HashMap::new();
    for p in &prepared {
        if let (Some(name), Some(cluster), Some((s, e))) = (&p.own_name, p.dominant, p.times) {
            *votes.entry(cluster).or_default().entry(name.clone()).or_default() += e - s;
        }
    }
    let inherited: HashMap<usize, String> = votes
        .into_iter()
        .filter_map(|(cluster, names)| {
            // BTreeMap iterates names ascending, so strict `>` keeps the smaller on ties.
            let mut best: Option<(String, f64)> = None;
            for (name, secs) in names {
                if best.as_ref().map_or(true, |(_, b)| secs > *b) {
                    best = Some((name, secs));
                }
            }
            best.map(|(name, _)| (cluster, name))
        })
        .collect();

    let speaker_label = |spk: usize| -> String {
        if let Some(name) = inherited.get(&spk) {
            return name.clone();
        }
        if Some(spk) == user_speaker {
            return "You".to_string();
        }
        // Keep remote numbering compact when the user owns a cluster.
        let display = match user_speaker {
            Some(user) if spk > user => spk,
            _ => spk + 1,
        };
        format!("Speaker {}", display)
    };

    prepared
        .into_iter()
        .map(|p| {
            let Prepared { row, track, times, turns: row_turns, own_name, dominant } = p;
            let existing = row.speaker.as_deref().map(str::trim).filter(|l| !l.is_empty());
            let named = existing.map_or(false, is_named);
            let Some((start, end)) = times else {
                return if named {
                    RowPlan::Untouched { id: row.id, label: existing.map(str::to_string) }
                } else {
                    RowPlan::Speaker { id: row.id, label: None }
                };
            };
            // Combined custom labels can't be attributed to pieces; leave them.
            if named && own_name.is_none() {
                return RowPlan::Untouched { id: row.id, label: existing.map(str::to_string) };
            }

            let words = row.words.clone().unwrap_or_default();
            let pieces = split_row(start, end, &words, &row_turns);

            let label_of = |piece: &Piece| -> Option<String> {
                if piece.speakers.is_empty() {
                    if named {
                        return existing.map(str::to_string);
                    }
                    return match track {
                        Track::Mic if used_source_tracks => Some("You".to_string()),
                        Track::System if used_source_tracks && allow_remote_source_hint => {
                            Some("Guest".to_string())
                        }
                        _ => None,
                    };
                }
                let mut labels: Vec<String> = piece
                    .speakers
                    .iter()
                    .enumerate()
                    .map(|(i, &spk)| match (&own_name, dominant) {
                        (Some(name), Some(d)) if i == 0 && spk == d => name.clone(),
                        _ => speaker_label(spk),
                    })
                    .collect();
                if let Some(pos) = labels.iter().position(|l| l.eq_ignore_ascii_case("you")) {
                    let you = labels.remove(pos);
                    labels.insert(0, you);
                }
                let mut seen: Vec<String> = Vec::new();
                for l in labels {
                    if !seen.contains(&l) {
                        seen.push(l);
                    }
                }
                seen.truncate(MAX_COMBINED_SPEAKERS);
                Some(seen.join(" + "))
            };

            // Merge neighbours that ended up with the same label (two clusters
            // can inherit one name).
            let mut merged: Vec<(Piece, Option<String>)> = Vec::new();
            for piece in pieces {
                let label = label_of(&piece);
                match merged.last_mut() {
                    Some((last, last_label)) if *last_label == label => {
                        last.end = piece.end;
                        last.words.end = piece.words.end;
                    }
                    _ => merged.push((piece, label)),
                }
            }

            if merged.len() == 1 {
                return RowPlan::Speaker { id: row.id, label: merged.remove(0).1 };
            }
            let pieces = merged
                .into_iter()
                .map(|(piece, label)| {
                    let slice = &words[piece.words.clone()];
                    let text = slice.iter().map(|w| w.text.trim()).collect::<Vec<_>>().join(" ");
                    PlannedPiece {
                        start: piece.start,
                        end: piece.end,
                        text: Some(text),
                        words: Some(slice.to_vec()),
                        label,
                    }
                })
                .collect();
            RowPlan::Split { id: row.id, source: row.source, pieces }
        })
        .collect()
}

/// Writes the plans inside the caller's transaction. Returns `(row id, label)`
/// for every row that ends up with a speaker, including newly inserted rows.
pub async fn apply_plans(
    conn: &mut SqliteConnection,
    meeting_id: &str,
    recording_started_at: DateTime<Utc>,
    plans: Vec<RowPlan>,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    use crate::database::repositories::transcript::timestamp_from_offset;

    let mut assignments = Vec::new();
    for plan in plans {
        match plan {
            RowPlan::Untouched { id, label } => {
                if let Some(label) = label {
                    assignments.push((id, label));
                }
            }
            RowPlan::Speaker { id, label } => {
                sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
                    .bind(&label)
                    .bind(&id)
                    .execute(&mut *conn)
                    .await?;
                if let Some(label) = label {
                    assignments.push((id, label));
                }
            }
            RowPlan::Split { id, source, pieces } => {
                for (i, piece) in pieces.into_iter().enumerate() {
                    let words_json = piece
                        .words
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()
                        .map_err(|e| sqlx::Error::Protocol(format!("words json: {e}")))?;
                    let timestamp = timestamp_from_offset(recording_started_at, piece.start)?;
                    let duration = piece.end - piece.start;
                    let row_id = if i == 0 {
                        sqlx::query(
                            "UPDATE transcripts SET transcript = ?, timestamp = ?, \
                             audio_start_time = ?, audio_end_time = ?, duration = ?, \
                             speaker = ?, words = ? WHERE id = ?",
                        )
                        .bind(&piece.text)
                        .bind(&timestamp)
                        .bind(piece.start)
                        .bind(piece.end)
                        .bind(duration)
                        .bind(&piece.label)
                        .bind(&words_json)
                        .bind(&id)
                        .execute(&mut *conn)
                        .await?;
                        id.clone()
                    } else {
                        let new_id = format!("transcript-{}", Uuid::new_v4());
                        sqlx::query(
                            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, \
                             audio_start_time, audio_end_time, duration, speaker, words, source) \
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        )
                        .bind(&new_id)
                        .bind(meeting_id)
                        .bind(&piece.text)
                        .bind(&timestamp)
                        .bind(piece.start)
                        .bind(piece.end)
                        .bind(duration)
                        .bind(&piece.label)
                        .bind(&words_json)
                        .bind(&source)
                        .execute(&mut *conn)
                        .await?;
                        new_id
                    };
                    if let Some(label) = piece.label {
                        assignments.push((row_id, label));
                    }
                }
            }
        }
    }
    Ok(assignments)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start: f64, end: f64, speaker: usize) -> Turn {
        Turn { start, end, speaker }
    }

    fn w(text: &str, start: f64, end: f64) -> WordTiming {
        WordTiming { text: text.to_string(), start, end }
    }

    fn row(id: &str, start: f64, end: f64, speaker: Option<&str>, source: Option<&str>) -> RowInput {
        RowInput {
            id: id.to_string(),
            start: Some(start),
            end: Some(end),
            speaker: speaker.map(str::to_string),
            words: None,
            source: source.map(str::to_string),
        }
    }

    fn speaker_of(plan: &RowPlan) -> Option<String> {
        match plan {
            RowPlan::Speaker { label, .. } | RowPlan::Untouched { label, .. } => label.clone(),
            other => panic!("expected single label, got {other:?}"),
        }
    }

    /// Words at 1s each, texts w0..w{n-1}, starting at `start`.
    fn words(start: f64, n: usize) -> Vec<WordTiming> {
        (0..n).map(|i| w(&format!("w{i}"), start + i as f64, start + i as f64 + 1.0)).collect()
    }

    #[test]
    fn dual_system_row_ignores_overlapping_mic_speech() {
        let turns = [turn(0.0, 10.0, 0), turn(0.0, 10.0, 1)];
        let plans = plan_rows(
            vec![row("a", 0.0, 10.0, None, Some("system"))],
            &turns,
            Some(0),
            true,
            true,
        );
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn dual_rows_without_own_track_turns_fall_back_to_source_hint() {
        // Quiet mic speech can miss segmentation; the track still says who spoke.
        let turns = [turn(0.0, 10.0, 1)];
        let plans = plan_rows(
            vec![
                row("mic", 0.0, 10.0, Some("You"), Some("mic")),
                row("sys", 20.0, 30.0, None, Some("system")),
            ],
            &turns,
            Some(0),
            true,
            true,
        );
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("You"));
        assert_eq!(speaker_of(&plans[1]).as_deref(), Some("Guest"));
    }

    #[test]
    fn dual_mic_row_is_you_despite_system_overlap() {
        let turns = [turn(0.0, 10.0, 0), turn(0.0, 10.0, 1)];
        let plans = plan_rows(
            vec![row("a", 0.0, 10.0, None, Some("mic"))],
            &turns,
            Some(0),
            true,
            true,
        );
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("You"));
    }

    #[test]
    fn row_with_words_splits_at_speaker_change() {
        let turns = [turn(0.0, 4.0, 1), turn(4.0, 8.0, 2)];
        let mut r = row("a", 0.0, 8.0, None, Some("system"));
        r.words = Some(words(0.0, 8));
        let plans = plan_rows(vec![r], &turns, Some(0), true, true);
        let RowPlan::Split { id, source, pieces } = &plans[0] else {
            panic!("expected split: {:?}", plans[0]);
        };
        assert_eq!(id, "a");
        assert_eq!(source.as_deref(), Some("system"));
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].label.as_deref(), Some("Speaker 1"));
        assert_eq!(pieces[1].label.as_deref(), Some("Speaker 2"));
        assert_eq!(pieces[0].text.as_deref(), Some("w0 w1 w2 w3"));
        assert_eq!(pieces[1].text.as_deref(), Some("w4 w5 w6 w7"));
        assert_eq!(pieces[0].words.as_ref().unwrap().len(), 4);
        assert_eq!(pieces[0].start, 0.0);
        assert_eq!(pieces[0].end, pieces[1].start);
        assert_eq!(pieces[1].end, 8.0);
    }

    #[test]
    fn row_without_words_is_not_split_and_uses_own_track() {
        let turns = [turn(0.0, 5.0, 1), turn(5.0, 10.0, 2), turn(0.0, 10.0, 0)];
        let plans = plan_rows(
            vec![row("a", 0.0, 10.0, None, Some("system"))],
            &turns,
            Some(0),
            true,
            true,
        );
        assert!(matches!(plans[0], RowPlan::Speaker { .. }));
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("Speaker 1"));
    }

    #[test]
    fn cluster_inherits_name_by_vote() {
        let turns = [turn(0.0, 20.0, 1)];
        let plans = plan_rows(
            vec![
                row("a", 0.0, 10.0, Some("Matheus"), Some("system")),
                row("b", 10.0, 14.0, Some("Ana"), Some("system")),
                row("c", 14.0, 20.0, Some("Speaker 1"), Some("system")),
            ],
            &turns,
            Some(0),
            true,
            true,
        );
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("Matheus"));
        assert_eq!(speaker_of(&plans[2]).as_deref(), Some("Matheus"));
    }

    #[test]
    fn named_row_with_two_speakers_keeps_own_name_on_dominant_piece() {
        // Cluster 1 -> Matheus (dominant of row "a"), cluster 2 unnamed.
        let turns = [turn(0.0, 6.0, 1), turn(6.0, 8.0, 2), turn(8.0, 30.0, 3)];
        let mut r = row("a", 0.0, 8.0, Some("Matheus"), Some("system"));
        r.words = Some(words(0.0, 8));
        let plans = plan_rows(
            vec![r, row("b", 8.0, 30.0, Some("Rita"), Some("system"))],
            &turns,
            Some(0),
            true,
            true,
        );
        let RowPlan::Split { pieces, .. } = &plans[0] else {
            panic!("expected split: {:?}", plans[0]);
        };
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].label.as_deref(), Some("Matheus"));
        assert_eq!(pieces[1].label.as_deref(), Some("Speaker 2"));
        assert_eq!(speaker_of(&plans[1]).as_deref(), Some("Rita"));
    }

    #[test]
    fn adjacent_pieces_with_same_label_merge() {
        // Two clusters both inherit "Matheus".
        let turns = [turn(0.0, 4.0, 1), turn(4.0, 8.0, 2)];
        let mut a = row("a", 0.0, 8.0, Some("Matheus"), Some("system"));
        a.words = Some(words(0.0, 8));
        let plans = plan_rows(
            vec![
                a,
                row("b", 0.0, 4.0, Some("Matheus"), Some("system")),
                row("c", 4.0, 8.0, Some("Matheus"), Some("system")),
            ],
            &turns,
            Some(0),
            true,
            true,
        );
        assert!(matches!(plans[0], RowPlan::Speaker { .. }));
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("Matheus"));
    }

    #[test]
    fn no_remote_hint_when_single_speaker() {
        let plans = plan_rows(
            vec![row("a", 0.0, 5.0, None, Some("system"))],
            &[turn(0.0, 5.0, 0)],
            None,
            true,
            false,
        );
        assert_eq!(speaker_of(&plans[0]), None);
    }

    #[test]
    fn rows_without_times() {
        let mut named = row("a", 0.0, 0.0, Some("Matheus"), None);
        named.start = None;
        let mut generated = row("b", 0.0, 0.0, Some("Speaker 1"), None);
        generated.end = None;
        let plans = plan_rows(vec![named, generated], &[], None, false, true);
        assert_eq!(
            plans[0],
            RowPlan::Untouched { id: "a".into(), label: Some("Matheus".into()) }
        );
        assert_eq!(plans[1], RowPlan::Speaker { id: "b".into(), label: None });
    }

    #[test]
    fn renamed_user_mic_rows_do_not_get_you_prefixed() {
        let turns = [turn(0.0, 20.0, 0), turn(0.0, 20.0, 1)];
        let plans = plan_rows(
            vec![
                row("a", 0.0, 10.0, Some("Felipe"), Some("mic")),
                row("b", 10.0, 15.0, Some("You"), Some("mic")),
            ],
            &turns,
            Some(0),
            true,
            true,
        );
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("Felipe"));
        assert_eq!(speaker_of(&plans[1]).as_deref(), Some("Felipe"));
    }

    #[test]
    fn null_source_with_guest_label_is_system_in_dual_mode() {
        let turns = [turn(0.0, 10.0, 0), turn(0.0, 10.0, 1)];
        let plans = plan_rows(
            vec![row("a", 0.0, 10.0, Some("Guest"), None)],
            &turns,
            Some(0),
            true,
            true,
        );
        assert_eq!(speaker_of(&plans[0]).as_deref(), Some("Speaker 1"));
    }

    #[tokio::test]
    async fn apply_split_plan_inserts_second_row() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let started: DateTime<Utc> = "2026-01-01T10:00:00Z".parse().unwrap();
        sqlx::query(
            "INSERT INTO meetings (id, title, created_at, updated_at) \
             VALUES ('m1', 't', '2026-01-01T10:00:00Z', '2026-01-01T10:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, \
             audio_end_time, duration, speaker, source) \
             VALUES ('r1', 'm1', 'w0 w1 w2 w3', '2026-01-01T10:00:00Z', 0, 4, 4, 'Speaker 1', 'system')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let ws = words(0.0, 4);
        let plans = vec![RowPlan::Split {
            id: "r1".into(),
            source: Some("system".into()),
            pieces: vec![
                PlannedPiece {
                    start: 0.0,
                    end: 2.0,
                    text: Some("w0 w1".into()),
                    words: Some(ws[..2].to_vec()),
                    label: Some("Speaker 1".into()),
                },
                PlannedPiece {
                    start: 2.0,
                    end: 4.0,
                    text: Some("w2 w3".into()),
                    words: Some(ws[2..].to_vec()),
                    label: Some("Speaker 2".into()),
                },
            ],
        }];
        let mut tx = pool.begin().await.unwrap();
        let assignments = apply_plans(&mut tx, "m1", started, plans).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(assignments.len(), 2);

        type Row = (String, String, String, f64, f64, f64, Option<String>, Option<String>, Option<String>);
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT id, transcript, timestamp, audio_start_time, audio_end_time, duration, \
             speaker, words, source FROM transcripts WHERE meeting_id = 'm1' ORDER BY audio_start_time",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "r1");
        assert_ne!(rows[1].0, "r1");
        assert!(rows[1].0.starts_with("transcript-"));
        assert_eq!(rows[0].1, "w0 w1");
        assert_eq!(rows[1].1, "w2 w3");
        assert_eq!((rows[1].3, rows[1].4, rows[1].5), (2.0, 4.0, 2.0));
        assert!(rows[1].2.starts_with("2026-01-01T10:00:02"));
        assert_eq!(rows[0].6.as_deref(), Some("Speaker 1"));
        assert_eq!(rows[1].6.as_deref(), Some("Speaker 2"));
        assert_eq!(rows[1].8.as_deref(), Some("system"));
        let parsed: Vec<WordTiming> = serde_json::from_str(rows[1].7.as_deref().unwrap()).unwrap();
        assert_eq!(parsed, ws[2..].to_vec());
    }
}
