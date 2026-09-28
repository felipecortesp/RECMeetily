//! Per-word timings derived from token-level timestamps (Parakeet TDT).

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WordTiming {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Groups tokens into words. Tokens carry a leading space when they begin a new word
/// (SentencePiece `▁` already replaced); subwords and punctuation continue the previous one.
/// A word ends where the next begins; the last one ends at `audio_duration`.
pub fn words_from_tokens(
    tokens: &[String],
    token_starts: &[f32],
    audio_duration: f64,
) -> Vec<WordTiming> {
    let len = tokens.len().min(token_starts.len());

    // (text, start) per word, before end times are known
    let mut raw: Vec<(String, f64)> = Vec::new();
    for i in 0..len {
        let starts_word = i == 0 || tokens[i].starts_with(char::is_whitespace);
        if starts_word {
            raw.push((tokens[i].clone(), token_starts[i] as f64));
        } else if let Some(last) = raw.last_mut() {
            last.0.push_str(&tokens[i]);
        }
    }

    let words: Vec<(String, f64)> = raw
        .into_iter()
        .filter(|(text, _)| !text.trim().is_empty())
        .collect();

    let mut out = Vec::with_capacity(words.len());
    for (i, (text, start)) in words.iter().enumerate() {
        let end = words.get(i + 1).map_or(audio_duration, |next| next.1);
        let end = end.min(audio_duration);
        let start = start.min(end);
        out.push(WordTiming {
            text: text.trim().to_string(),
            start,
            end,
        });
    }
    out
}

/// Shifts word times, e.g. from chunk-relative to recording-relative.
pub fn offset_words(words: &[WordTiming], offset_secs: f64) -> Vec<WordTiming> {
    words
        .iter()
        .map(|w| WordTiming {
            text: w.text.clone(),
            start: w.start + offset_secs,
            end: w.end + offset_secs,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn w(text: &str, start: f64, end: f64) -> WordTiming {
        WordTiming { text: text.to_string(), start, end }
    }

    #[test]
    fn empty_input_gives_no_words() {
        assert!(words_from_tokens(&[], &[], 5.0).is_empty());
    }

    #[test]
    fn subword_tokens_form_single_word() {
        let r = words_from_tokens(&toks(&[" hel", "lo"]), &[0.5, 0.75], 2.0);
        assert_eq!(r, vec![w("hello", 0.5, 2.0)]);
    }

    #[test]
    fn two_words_chain_start_and_end() {
        let r = words_from_tokens(&toks(&[" hi", " there"]), &[0.0, 1.0], 3.0);
        assert_eq!(r, vec![w("hi", 0.0, 1.0), w("there", 1.0, 3.0)]);
    }

    #[test]
    fn punctuation_attaches_to_previous_word() {
        let r = words_from_tokens(&toks(&[" ok", ".", " yes"]), &[0.0, 0.5, 1.0], 2.0);
        assert_eq!(r, vec![w("ok.", 0.0, 1.0), w("yes", 1.0, 2.0)]);
    }

    #[test]
    fn first_token_without_space_starts_word() {
        let r = words_from_tokens(&toks(&["ab", "c", " d"]), &[0.25, 0.5, 1.0], 2.0);
        assert_eq!(r, vec![w("abc", 0.25, 1.0), w("d", 1.0, 2.0)]);
    }

    #[test]
    fn last_word_ends_at_audio_duration() {
        let r = words_from_tokens(&toks(&[" a", " b"]), &[0.0, 1.0], 4.5);
        assert_eq!(r.last().unwrap().end, 4.5);
    }

    #[test]
    fn length_mismatch_uses_shorter() {
        let r = words_from_tokens(&toks(&[" a", " b", " c"]), &[0.0, 1.0], 3.0);
        assert_eq!(r, vec![w("a", 0.0, 1.0), w("b", 1.0, 3.0)]);
        let r = words_from_tokens(&toks(&[" a"]), &[0.0, 1.0, 2.0], 3.0);
        assert_eq!(r, vec![w("a", 0.0, 3.0)]);
    }

    #[test]
    fn start_beyond_duration_is_clamped() {
        let r = words_from_tokens(&toks(&[" a", " b"]), &[0.0, 9.0], 2.0);
        assert_eq!(r, vec![w("a", 0.0, 2.0), w("b", 2.0, 2.0)]);
    }

    #[test]
    fn offset_words_shifts_both_fields() {
        let r = offset_words(&[w("a", 0.5, 1.0)], 10.0);
        assert_eq!(r, vec![w("a", 10.5, 11.0)]);
    }
}
