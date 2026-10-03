#[derive(Debug, Clone, PartialEq)]
pub struct SourceSnapshot {
    /// Most recent finalized utterance, used as disambiguation context only.
    pub committed: String,
    /// The current mutable interim (or finalized) transcript text.
    pub current: String,
    pub is_final: bool,
    /// Stream-relative offset (seconds) of the first word, used for E2E latency.
    pub word_start: Option<f64>,
}

use std::time::{Duration, Instant};

/// ASR packet boundaries are not translation boundaries. Only confirmed words
/// enter history; the entire open sentence (stable + interim) is retranslated.
#[derive(Debug, Default)]
pub struct TranscriptReconciler {
    pub committed: String,
    pub last_committed: String,
    sentence_buffer: String,
    current_utterance: String,
    last_text_at: Option<Instant>,
}

impl TranscriptReconciler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        transcript: &str,
        is_final: bool,
        _speech_final: bool,
    ) -> Option<SourceSnapshot> {
        let text = transcript.trim();
        // Empty Results and UtteranceEnd are transport/pause hints. They must
        // not cut an unfinished sentence, or flush behind a newer interim.
        if text.is_empty() {
            return None;
        }
        self.last_text_at = Some(Instant::now());
        if !is_final && text == self.current_utterance {
            return None;
        }

        // A following independent sentence confirms a prior punctuation mark.
        // Short fragments and explicit continuations remain in the same unit.
        let completed = if self.current_utterance.is_empty()
            && sentence_boundary(&self.sentence_buffer)
            && !is_continuation(text)
        {
            self.flush()
        } else {
            None
        };

        if is_final {
            if !self.sentence_buffer.is_empty() {
                self.sentence_buffer.push(' ');
            }
            self.sentence_buffer.push_str(text);
            self.current_utterance.clear();
        } else {
            self.current_utterance = text.to_owned();
        }
        self.last_text_at = Some(Instant::now());
        if completed.is_some() {
            return completed;
        }
        // Safety bound for unpunctuated monologues, not a 12-word caption
        // budget. Never subdivide one confirmed ASR packet a second time.
        if is_final
            && (self.sentence_buffer.split_whitespace().count() >= 72
                || self.sentence_buffer.chars().count() >= 600)
        {
            return self.flush();
        }
        self.preview()
    }

    /// Force-drain confirmed words only, on stop/disconnect. The unfinished
    /// hypothesis is retained separately and saved as unconfirmed by the caller.
    pub fn flush(&mut self) -> Option<SourceSnapshot> {
        if self.sentence_buffer.is_empty() {
            return None;
        }
        let completed = std::mem::take(&mut self.sentence_buffer);
        let previous = std::mem::replace(&mut self.last_committed, completed.clone());
        if !self.committed.is_empty() {
            self.committed.push(' ');
        }
        self.committed.push_str(&completed);
        if self.committed.len() > 8000 {
            self.committed = self
                .committed
                .chars()
                .rev()
                .take(4000)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
        }
        Some(SourceSnapshot {
            committed: previous,
            current: completed,
            is_final: true,
            word_start: None,
        })
    }

    pub fn take_interim(&mut self) -> Option<String> {
        let text = std::mem::take(&mut self.current_utterance);
        (!text.is_empty()).then_some(text)
    }

    pub fn flush_due(&mut self) -> Option<SourceSnapshot> {
        self.flush_at(Instant::now())
    }

    fn flush_at(&mut self, now: Instant) -> Option<SourceSnapshot> {
        if !self.current_utterance.is_empty() {
            return None;
        }
        let quiet = now.saturating_duration_since(self.last_text_at?);
        // Give ASR punctuation a short look-ahead. A 300ms VAD endpoint is not
        // a sentence end. Without punctuation, wait for a substantial pause.
        if quiet >= Duration::from_millis(1800)
            || (quiet >= Duration::from_millis(650) && sentence_boundary(&self.sentence_buffer))
        {
            self.flush()
        } else {
            None
        }
    }

    pub fn preview(&self) -> Option<SourceSnapshot> {
        let current = [
            self.sentence_buffer.as_str(),
            self.current_utterance.as_str(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        (!current.is_empty()).then(|| SourceSnapshot {
            committed: self.last_committed.clone(),
            current,
            is_final: false,
            word_start: None,
        })
    }
}

fn sentence_boundary(text: &str) -> bool {
    let text = text.trim_end_matches([' ', '"', '\'', '”', '»', ')']);
    if !text.ends_with(['.', '?', '!', '。', '？', '！']) || text.ends_with("...") {
        return false;
    }
    let words: Vec<_> = text.split_whitespace().collect();
    let last = words.last().copied().unwrap_or("").to_lowercase();
    if matches!(
        last.as_str(),
        "mr." | "mrs." | "ms." | "dr." | "prof." | "e.g." | "i.e." | "vs." | "etc."
    ) || (last.ends_with('.') && last.chars().filter(|c| c.is_alphabetic()).count() == 1)
    {
        return false;
    }
    words.len() >= 4
        || (text.chars().any(|c| ('\u{3000}'..='\u{9fff}').contains(&c))
            && text.chars().count() >= 8)
}

fn is_continuation(text: &str) -> bool {
    let first = text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphabetic())
        .to_lowercase();
    matches!(
        first.as_str(),
        "and"
            | "but"
            | "or"
            | "which"
            | "that"
            | "because"
            | "although"
            | "whereas"
            | "whose"
            | "и"
            | "но"
            | "или"
            | "который"
            | "которая"
            | "которое"
            | "которые"
            | "что"
            | "потому"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quiet(r: &mut TranscriptReconciler, ms: u64) -> Option<SourceSnapshot> {
        r.flush_at(r.last_text_at.unwrap() + Duration::from_millis(ms))
    }
    #[test]
    fn screenshot_regression_keeps_brain_waves_and_memory_benefit_together() {
        let mut r = TranscriptReconciler::new();
        let parts = [
            "not only can we amplify the size of those deep sleep brain",
            "waves,",
            "but in doing so we can almost double the amount of memory",
            "benefit",
            "that you get from sleep.",
        ];
        for part in parts {
            assert!(!r.update(part, true, true).unwrap().is_final);
            assert!(quiet(&mut r, 300).is_none());
        }
        let sentence = quiet(&mut r, 650).unwrap();
        assert_eq!(sentence.current, parts.join(" "));
        assert!(sentence.is_final);
        assert!(r.preview().is_none());
        assert!(r.flush().is_none());
    }
    #[test]
    fn short_asr_sentences_and_relative_clause_do_not_become_orphans() {
        let mut r = TranscriptReconciler::new();
        for part in [
            "The question now.",
            "Is whether it is possible to transfer this",
            "same affordable, potentially portable piece of technology into people's homes.",
        ] {
            assert!(!r.update(part, true, true).unwrap().is_final);
        }
        assert_eq!(quiet(&mut r, 650).unwrap().current, "The question now. Is whether it is possible to transfer this same affordable, potentially portable piece of technology into people's homes.");
    }
    #[test]
    fn timer_never_seals_a_prefix_while_its_continuation_is_being_recognized() {
        let mut r = TranscriptReconciler::new();
        r.update("This part is stable", true, true);
        r.update("and this part is changing", false, false);
        assert!(quiet(&mut r, 10000).is_none());
        assert!(r.update("", true, true).is_none());
        assert_eq!(
            r.preview().unwrap().current,
            "This part is stable and this part is changing"
        );
    }
    #[test]
    fn quiet_short_response_is_not_lost_and_interim_is_never_committed() {
        let mut r = TranscriptReconciler::new();
        r.update("Yes.", true, true);
        assert!(quiet(&mut r, 650).is_none());
        assert_eq!(quiet(&mut r, 1800).unwrap().current, "Yes.");
        r.update("I can approve", false, false);
        r.update("I cannot approve", false, false);
        assert!(quiet(&mut r, 10000).is_none());
        assert!(r.flush().is_none());
        assert_eq!(r.take_interim().as_deref(), Some("I cannot approve"));
    }
    #[test]
    fn next_sentence_can_commit_previous_without_losing_new_interim() {
        let mut r = TranscriptReconciler::new();
        r.update("This is the first sentence.", true, true);
        let first = r.update("Here is another", false, false).unwrap();
        assert!(first.is_final);
        assert_eq!(first.current, "This is the first sentence.");
        let next = r.preview().unwrap();
        assert_eq!(next.current, "Here is another");
        assert_eq!(next.committed, first.current);
        r.update("Here is another sentence.", true, true);
        assert_eq!(
            quiet(&mut r, 650).unwrap().current,
            "Here is another sentence."
        );
    }
    #[test]
    fn repeated_identical_sentences_are_not_deduplicated() {
        let mut r = TranscriptReconciler::new();
        for _ in 0..2 {
            r.update("This is the same sentence.", true, true);
            assert!(quiet(&mut r, 650).unwrap().is_final);
        }
        assert_eq!(
            r.committed,
            "This is the same sentence. This is the same sentence."
        );
    }
    #[test]
    fn abbreviations_commas_and_semicolons_are_not_sentence_boundaries() {
        for text in [
            "I spoke to Dr.",
            "The CEO is A.",
            "There is more to come,",
            "There is more to come;",
            "There is more to come...",
        ] {
            assert!(!sentence_boundary(text), "{text}");
        }
        assert!(sentence_boundary("This is a complete sentence.\""));
        assert!(sentence_boundary("日本語の文章を翻訳します。"));
    }
    #[test]
    fn long_unpunctuated_speech_has_a_safety_bound_without_word_loss() {
        let mut r = TranscriptReconciler::new();
        let text = "word ".repeat(72).trim().to_string();
        assert_eq!(r.update(&text, true, false).unwrap().current, text);
        assert!(r.preview().is_none());
        let text = "日".repeat(600);
        assert!(r.update(&text, true, false).unwrap().is_final);
    }
    #[test]
    fn shutdown_drains_stable_prefix_and_preserves_unconfirmed_tail() {
        let mut r = TranscriptReconciler::new();
        r.update("Stable prefix", true, false);
        r.update("unfinished tail", false, false);
        assert_eq!(r.flush().unwrap().current, "Stable prefix");
        assert_eq!(r.take_interim().as_deref(), Some("unfinished tail"));
        assert!(r.take_interim().is_none());
    }
}
