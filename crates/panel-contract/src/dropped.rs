//! What the reader let fall on the way in, **said once, as the sentence a producer reads**.
//!
//! The document is read tolerantly per field: a word none of the three closed vocabularies knows —or a
//! value that is not even a word— is dropped and the panel still paints. That stays. What this module
//! adds is the record of those three fields, so the one transport with somebody on the other end can
//! say which of their values fell away (**D58**). It records nothing else the reader drops or clips,
//! and the file transport keeps reading silently: nobody is there to be told.
//!
//! **Its only public output is a sentence, not a structure.** The one consumer turns it into text for
//! an agent, and the contract fixes the words the way it fixes [`Problem::Refused`]'s — a structure on
//! the wire would be a second contract to keep in step with this one.
//!
//! [`Problem::Refused`]: crate::Problem::Refused

use std::io;

/// How many distinct values of one field are named before the rest become «and others».
///
/// **It is a bound on the reply, not tidiness.** Every value is escaped once into the sentence and
/// again into the line that carries it, and the client refuses a reply past `MAX_BYTES` plus one
/// kibibyte: a document of many short distinct garbage marks, legal going in, would otherwise produce
/// a reply the client drops as an error — and dropping it drops the pane's claim with it.
const NAMED: usize = 3;

/// How many characters of one value are shown before it is cut, for the same ceiling as [`NAMED`]:
/// every character is escaped twice on its way back — a quote grows to four bytes, a control character
/// written as `\u0001` to seven — so a single mark of a few hundred kibibytes of them, legal going in,
/// would come back several times over the ceiling.
const SHOWN: usize = 32;

/// What the reader dropped from one document, or from the lines one patch carried.
///
/// A clean document leaves it empty, and an empty one says nothing: [`Dropped::sentence`] is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dropped {
    /// The document's own field, so there is at most one of it and no lines to count.
    attention: Option<String>,
    mark: Losses,
    style: Losses,
}

/// The three fields whose value is a closed vocabulary — the only ones this module records.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Vocabulary {
    Attention,
    Mark,
    Style,
}

/// What one line field lost: on how many lines, and which values, as shown, in the order they arrived.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Losses {
    lines: usize,
    seen: Vec<String>,
    more: bool,
}

impl Dropped {
    /// Records that `value` was found in `field` and understood as nothing.
    ///
    /// **A null is never passed here**: it reads exactly like the field left out, which is not a loss.
    pub(crate) fn note(&mut self, field: Vocabulary, value: &serde_json::Value) {
        let losses = match field {
            Vocabulary::Attention => {
                if self.attention.is_none() {
                    self.attention = Some(shown(value));
                }
                return;
            }
            Vocabulary::Mark => &mut self.mark,
            Vocabulary::Style => &mut self.style,
        };
        losses.lines += 1;
        // Past the cap nothing more is kept, so nothing more is rendered. Until then every value is
        // rendered to be compared with the ones already named, each within the cut.
        if losses.more {
            return;
        }
        let shown = shown(value);
        if losses.seen.contains(&shown) {
            return;
        }
        if losses.seen.len() < NAMED {
            losses.seen.push(shown);
        } else {
            losses.more = true;
        }
    }

    /// What fell away, in the words the reply carries — or `None` when nothing did.
    ///
    /// The fields always come in the same order —attention, mark, style— so the same document always
    /// reads the same.
    #[must_use]
    pub fn sentence(&self) -> Option<String> {
        let parts: Vec<String> = [
            self.attention
                .as_ref()
                .map(|value| format!("the document carries an unknown attention {value}")),
            self.mark.said("mark"),
            self.style.said("style"),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

impl Losses {
    fn said(&self, noun: &str) -> Option<String> {
        let (last, init) = self.seen.split_last()?;
        let carry = if self.lines == 1 {
            "1 line carries".to_owned()
        } else {
            format!("{} lines carry", self.lines)
        };
        let what = match (init, self.more) {
            ([], false) => format!("an unknown {noun} {last}"),
            (_, true) => format!("unknown {noun}s {} and others", self.seen.join(", ")),
            (init, false) => format!("unknown {noun}s {} and {last}", init.join(", ")),
        };
        Some(format!("{carry} {what}"))
    }
}

/// A value in compact JSON as the reader holds it — so `"doing"` keeps its quotes and `42` does not,
/// which is how a word is told apart from a number — and cut at [`SHOWN`] characters. What the reader
/// holds is the parsed value, not the producer's bytes, so an object or a number may not come back
/// spelled exactly as it was sent.
///
/// **A string is cut before it is quoted**, so the cut value still reads as one string; anything else
/// is cut on its compact JSON, which is **written only as far as the cut needs** — an array of a few
/// hundred kibibytes is not serialised whole to keep thirty-two characters of it — and whose cut may
/// land inside an escape, which the `…` still marks. Both cut on a character boundary, never on a byte.
/// Two values that agree up to the cut are named as one; each of their lines still counts.
fn shown(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(cut(text)).to_string(),
        other => {
            let mut head = Head::default();
            // The writer refuses once it holds enough, so an error here is the cut, not a failure.
            let _ = serde_json::to_writer(&mut head, other);
            cut(head.text())
        }
    }
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(SHOWN) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text.to_owned(),
    }
}

/// The start of a serialisation: enough bytes for [`SHOWN`] characters and one more, then a refusal.
#[derive(Default)]
struct Head(Vec<u8>);

impl Head {
    /// Four bytes is the widest a character is in UTF-8, and the one past the cut is what says a cut
    /// happened at all.
    const ROOM: usize = (SHOWN + 1) * 4;

    /// What was written, up to the last whole character.
    fn text(&self) -> &str {
        match std::str::from_utf8(&self.0) {
            Ok(text) => text,
            Err(broken) => std::str::from_utf8(&self.0[..broken.valid_up_to()]).unwrap_or_default(),
        }
    }
}

impl io::Write for Head {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let room = Self::ROOM.saturating_sub(self.0.len());
        if room == 0 {
            return Err(io::Error::other("enough to show"));
        }
        let taken = bytes.len().min(room);
        self.0.extend_from_slice(&bytes[..taken]);
        Ok(taken)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn noting(notes: &[(Vocabulary, serde_json::Value)]) -> Option<String> {
        let mut dropped = Dropped::default();
        for (field, value) in notes {
            dropped.note(*field, value);
        }
        dropped.sentence()
    }

    #[test]
    fn nothing_dropped_says_nothing() {
        assert_eq!(Dropped::default().sentence(), None);
    }

    /// The literal the issue asks for. Counting distinct values instead of lines would say «1 line».
    #[test]
    fn one_word_on_four_lines_is_one_word_and_four_lines() {
        let doing = || (Vocabulary::Mark, json!("doing"));

        assert_eq!(
            noting(&[doing(), doing(), doing(), doing()]).as_deref(),
            Some(r#"4 lines carry an unknown mark "doing""#)
        );
    }

    #[test]
    fn one_line_is_singular() {
        assert_eq!(
            noting(&[(Vocabulary::Style, json!("bold"))]).as_deref(),
            Some(r#"1 line carries an unknown style "bold""#)
        );
    }

    /// Four distinct values after a repeat, so a cap off by one — or a repeat counted as distinct —
    /// names a fourth.
    #[test]
    fn past_three_distinct_values_the_rest_are_others() {
        let marks = ["a", "b", "a", "c", "d"].map(|m| (Vocabulary::Mark, json!(m)));

        assert_eq!(
            noting(&marks).as_deref(),
            Some(r#"5 lines carry unknown marks "a", "b", "c" and others"#)
        );
    }

    #[test]
    fn two_and_three_values_are_listed_in_full() {
        let two = ["bold", "red"].map(|s| (Vocabulary::Style, json!(s)));
        let three = ["a", "b", "c"].map(|m| (Vocabulary::Mark, json!(m)));

        assert_eq!(
            noting(&two).as_deref(),
            Some(r#"2 lines carry unknown styles "bold" and "red""#)
        );
        assert_eq!(
            noting(&three).as_deref(),
            Some(r#"3 lines carry unknown marks "a", "b" and "c""#)
        );
    }

    /// The order is fixed by field, not by arrival: the notes come in reversed here.
    #[test]
    fn the_fields_are_said_attention_then_mark_then_style() {
        let sentence = noting(&[
            (Vocabulary::Style, json!("bold")),
            (Vocabulary::Style, json!("red")),
            (Vocabulary::Mark, json!("doing")),
            (Vocabulary::Attention, json!("waiting")),
        ]);

        assert_eq!(
            sentence.as_deref(),
            Some(
                r#"the document carries an unknown attention "waiting"; 1 line carries an unknown mark "doing"; 2 lines carry unknown styles "bold" and "red""#
            )
        );
    }

    /// Without the quotes an agent cannot tell the word `42` from the number.
    #[test]
    fn a_value_that_is_not_a_word_is_named_as_the_json_it_is() {
        assert_eq!(
            noting(&[(Vocabulary::Mark, json!(42))]).as_deref(),
            Some("1 line carries an unknown mark 42")
        );
        assert_eq!(
            noting(&[(Vocabulary::Style, json!({"a": 1}))]).as_deref(),
            Some(r#"1 line carries an unknown style {"a":1}"#)
        );
    }

    #[test]
    fn a_long_value_is_cut_at_thirty_two_characters_and_still_reads_as_a_string() {
        let long = "x".repeat(1000);

        assert_eq!(
            noting(&[(Vocabulary::Mark, json!(long))]).as_deref(),
            Some(format!(r#"1 line carries an unknown mark "{}…""#, "x".repeat(32)).as_str())
        );
    }

    /// Two bytes per character: a cut by bytes lands inside one and panics, or keeps sixteen.
    #[test]
    fn the_cut_counts_characters_not_bytes() {
        let accents = "é".repeat(40);

        assert_eq!(
            noting(&[(Vocabulary::Mark, json!(accents))]).as_deref(),
            Some(format!(r#"1 line carries an unknown mark "{}…""#, "é".repeat(32)).as_str())
        );
    }

    /// The cut is the reply's bound, so it has to hold for what is not a string too.
    #[test]
    fn a_long_value_that_is_not_a_word_is_cut_on_its_json() {
        let many = serde_json::Value::Array(vec![json!(1); 100_000]);

        assert_eq!(
            noting(&[(Vocabulary::Style, many)]).as_deref(),
            Some(format!("1 line carries an unknown style [{}1…", "1,".repeat(15)).as_str())
        );
    }

    /// Four bytes a character, so the partial serialisation stops inside the thirty-third emoji: a
    /// buffer sized in characters instead of bytes shows seven of them and no cut, and a partial
    /// character dropped wholesale instead of trimmed shows nothing at all.
    #[test]
    fn a_value_that_is_not_a_word_is_cut_on_whole_characters_however_wide() {
        let wide = json!(["😀".repeat(40)]);

        assert_eq!(
            noting(&[(Vocabulary::Mark, wide)]).as_deref(),
            Some(format!(r#"1 line carries an unknown mark ["{}…"#, "😀".repeat(30)).as_str())
        );
    }

    #[test]
    fn a_value_of_exactly_thirty_two_characters_is_not_cut() {
        let exact = "y".repeat(32);

        assert_eq!(
            noting(&[(Vocabulary::Mark, json!(exact))]).as_deref(),
            Some(format!(r#"1 line carries an unknown mark "{exact}""#).as_str())
        );
    }

    /// The reader, not the helper: the words have to reach the record through the document's own
    /// door, and the panel that door returns has to be the one the silent door returns.
    #[test]
    fn a_document_reports_what_it_dropped_and_paints_what_it_always_painted() {
        let document = r#"{"lines":[
            {"text":"one","mark":"doing"},
            {"text":"two","mark":"doing"},
            {"text":"three","mark":"doing"},
            {"text":"four","mark":"doing"},
            {"text":"five","mark":"pending"}]}"#;

        let (panel, dropped) = crate::parse_document_noting(document).expect("a panel");

        assert_eq!(
            dropped.sentence().as_deref(),
            Some(r#"4 lines carry an unknown mark "doing""#)
        );
        assert_eq!(
            panel,
            crate::parse_document(document).expect("the same panel")
        );
    }

    #[test]
    fn a_document_names_its_attention_its_numbers_and_not_its_nulls() {
        let document = r#"{"attention":"waiting","lines":[
            {"text":"a","mark":42},
            {"text":"b","mark":null,"style":null},
            {"text":"c","style":"bold"}]}"#;

        let (_, dropped) = crate::parse_document_noting(document).expect("a panel");

        assert_eq!(
            dropped.sentence().as_deref(),
            Some(
                r#"the document carries an unknown attention "waiting"; 1 line carries an unknown mark 42; 1 line carries an unknown style "bold""#
            )
        );
    }

    #[test]
    fn a_clean_document_reports_nothing() {
        let (_, dropped) = crate::parse_document_noting(
            r#"{"attention":"wait","lines":["x",{"text":"y","mark":"done","style":"ok"}]}"#,
        )
        .expect("a panel");

        assert_eq!(dropped.sentence(), None);
    }

    #[test]
    fn the_lines_a_patch_carries_report_what_they_dropped() {
        let ops = json!([
            {"op":"last","line":{"text":"a","mark":"doing"}},
            {"op":"first","line":{"text":"b","mark":"wip"}},
            {"op":"set","field":"phase","value":"build"}
        ]);

        let (patch, dropped) = crate::parse_ops_noting(&ops).expect("a patch");

        assert_eq!(
            dropped.sentence().as_deref(),
            Some(r#"2 lines carry unknown marks "doing" and "wip""#)
        );
        assert_eq!(patch, crate::parse_ops(&ops).expect("the same patch"));
    }
}
