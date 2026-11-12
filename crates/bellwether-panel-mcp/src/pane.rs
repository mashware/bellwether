//! This console's panel: the document the agent published, kept in memory for patching and written
//! to `<state>/<terminal>/panel.json`, which is what the Herdr mods read.
//!
//! **The file is the store, not a second transport.** In Ivy the window holds the document and the
//! file is how a producer without a channel publishes; here there is no window process to hold it, so
//! this server writes the file and the mods read it. That is also what lets `panel_get` recover a
//! panel after the agent restarts: the process dies, the file stays.

use std::io;
use std::path::{Path, PathBuf};

use panel_contract::{MAX_BYTES, Panel, Problem, parse_document_noting, parse_ops_noting, serialize};

/// The longest `source` taken, as in Ivy's channel: it keeps the reply bounded.
const MAX_SOURCE: usize = 256;

const NOTHING_YET: &str = "nothing has been published on this console yet";

/// What a verb answered, before it is worded as an MCP result.
#[derive(Debug, PartialEq)]
pub enum Answered {
    Ok,
    /// Applied, and some words of a closed vocabulary fell away: the sentence names them.
    Lossy(String),
    Document(String),
    Retired,
    Failed(String),
}

#[derive(Debug)]
pub struct Pane {
    dir: PathBuf,
    /// The source that claimed this pane with `panel_set`, for as long as this process lives.
    held: Option<String>,
    /// What was last committed, which a patch edits. It is never re-read from the file while
    /// held: a hold means this process is the pane's one writer, as in Ivy.
    document: Option<Panel>,
    /// Where Herdr's own status for this console goes; none in tests and outside a console.
    status: Option<crate::status::Sink>,
}

impl Pane {
    /// The pane of the console this process runs in, named by Herdr's `HC_TERMINAL_ID`.
    pub fn of_this_console() -> Result<Self, String> {
        let terminal = std::env::var("HC_TERMINAL_ID").unwrap_or_default();
        let base = state_base()?;
        let terminal = terminal_id(&terminal)?;
        let mut pane = Self::at(base.join(terminal));
        pane.status = Some(crate::status::Sink::new(terminal));
        Ok(pane)
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir, held: None, document: None, status: None }
    }

    fn tell_herdr(&mut self) {
        if let Some(sink) = &mut self.status {
            sink.publish(self.document.as_ref());
        }
    }

    fn file(&self) -> PathBuf {
        self.dir.join(panel_contract::PANEL_FILE)
    }

    pub fn publish(&mut self, source: &str, document: &serde_json::Value) -> Answered {
        if source.len() > MAX_SOURCE {
            return Answered::Failed(format!("a source takes at most {MAX_SOURCE} bytes"));
        }
        let (panel, dropped) = match parse_document_noting(&document.to_string()) {
            Ok(read) => read,
            Err(problem) => return Answered::Failed(problem.to_string()),
        };
        // **What was on screen before, even from a session that has since died**: an agent that
        // restarts and publishes the same `wait` is still the same wait, and its age must not reset.
        let before = self.document.clone().or_else(|| panel_contract::read(&self.dir).ok());
        let panel = stamped(before.as_ref(), panel, now());
        if let Err(why) = self.commit(&panel) {
            return Answered::Failed(why);
        }
        self.held = Some(source.to_owned());
        self.document = Some(panel);
        self.tell_herdr();
        lossy_or_ok(dropped.sentence())
    }

    pub fn read_the_document(&self) -> Answered {
        let panel = match &self.document {
            Some(panel) => panel.clone(),
            None => match panel_contract::read(&self.dir) {
                Ok(panel) => panel,
                Err(Problem::Missing) => return Answered::Failed(NOTHING_YET.to_owned()),
                Err(problem) => return Answered::Failed(problem.to_string()),
            },
        };
        match serialize(&panel) {
            Ok(text) => Answered::Document(text),
            Err(problem) => Answered::Failed(problem.to_string()),
        }
    }

    pub fn patch(&mut self, ops: &serde_json::Value) -> Answered {
        if ops.to_string().len() as u64 > MAX_BYTES {
            return Answered::Failed(format!("a patch's \"ops\" are over {} KiB", MAX_BYTES / 1024));
        }
        let Some(current) = &self.document else {
            return Answered::Failed(if self.file().exists() {
                "this console's panel is its file's: publish with panel_set before patching".to_owned()
            } else {
                NOTHING_YET.to_owned()
            });
        };
        let (patch, dropped) = match parse_ops_noting(ops) {
            Ok(read) => read,
            Err(problem) => return Answered::Failed(problem.to_string()),
        };
        let patched = match current.patched_with(&patch) {
            Ok(patched) => stamped(Some(current), patched, now()),
            Err(problem) => return Answered::Failed(problem.to_string()),
        };
        if let Err(why) = self.commit(&patched) {
            return Answered::Failed(why);
        }
        self.document = Some(patched);
        self.tell_herdr();
        lossy_or_ok(dropped.sentence())
    }

    /// Ends this session's panel: the file goes, and so does the hold. Only the process that
    /// published may do it, as in Ivy.
    pub fn retire(&mut self) -> Answered {
        if self.held.is_none() {
            return Answered::Failed(
                "only the session that published this panel can retire it".to_owned(),
            );
        }
        match std::fs::remove_file(self.file()) {
            Ok(()) => {}
            Err(failure) if failure.kind() == io::ErrorKind::NotFound => {}
            Err(failure) => return Answered::Failed(format!("could not remove the panel ({failure})")),
        }
        self.held = None;
        self.document = None;
        self.tell_herdr();
        Answered::Retired
    }

    /// Writes the document as it was understood, whole and at once: a temporary file in the same
    /// directory renamed over the panel, so the mods never read half of one.
    fn commit(&self, panel: &Panel) -> Result<(), String> {
        let text = serialize(panel).map_err(|problem| problem.to_string())?;
        write_atomically(&self.dir, &self.file(), &text)
            .map_err(|failure| format!("could not write the panel ({failure})"))
    }
}

/// The document with its `attention_since` set by this server, **never by the agent**.
///
/// The call keeps the moment it started for as long as it is the same call: a republish or a patch
/// that leaves `attention` as it was keeps the age the board orders by, and one that changes it
/// starts the clock again. No call, no moment. Whatever the agent wrote there is replaced: it is a
/// clock, and the one process that sees every change is the one to keep it.
fn stamped(before: Option<&Panel>, mut next: Panel, now: u64) -> Panel {
    next.attention_since = match (next.attention, before) {
        (None, _) => None,
        (Some(calling), Some(before)) if before.attention == Some(calling) => {
            before.attention_since.or(Some(now))
        }
        (Some(_), _) => Some(now),
    };
    next
}

/// Seconds since the Unix epoch, or zero on a clock set before it.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Where Bellwether keeps its state: `BELLWETHER_STATE`, or `~/.local/state/bellwether`.
pub fn state_base() -> Result<PathBuf, String> {
    match std::env::var_os("BELLWETHER_STATE") {
        Some(base) if !base.is_empty() => Ok(PathBuf::from(base)),
        _ => {
            let home = std::env::var_os("HOME")
                .filter(|home| !home.is_empty())
                .ok_or("there is no HOME to keep the panel in")?;
            Ok(PathBuf::from(home).join(".local/state/bellwether"))
        }
    }
}

fn lossy_or_ok(sentence: Option<String>) -> Answered {
    sentence.map_or(Answered::Ok, Answered::Lossy)
}

/// The terminal's id as a single path component, or why it is not one. Herdr's ids are like `t9`.
fn terminal_id(id: &str) -> Result<&str, String> {
    if id.is_empty() {
        return Err("this program is not running inside a Herdr console (HC_TERMINAL_ID is not set)".to_owned());
    }
    if id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(format!("HC_TERMINAL_ID {id:?} is not a terminal id"));
    }
    Ok(id)
}

/// Writes `text` to `file` whole and at once: a temporary file beside it, renamed over it.
pub fn write_atomically(dir: &Path, file: &Path, text: &str) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)?;
    let name = file.file_name().map_or_else(|| "file".into(), |name| name.to_string_lossy());
    let temporary = dir.join(format!(".{name}.{}", std::process::id()));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, file).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use panel_contract::Attention;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bellwether-panel-mcp-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn on_disk(dir: &Path) -> serde_json::Value {
        let text = std::fs::read_to_string(dir.join("panel.json")).expect("the panel was written");
        serde_json::from_str(&text).expect("the file is JSON")
    }

    #[test]
    fn a_publication_reaches_the_file_the_mods_read() {
        let dir = scratch("publish");
        let mut pane = Pane::at(dir.clone());

        let answered = pane.publish("flow", &json!({"attention": "wait", "lines": ["hello"]}));

        assert_eq!(answered, Answered::Ok);
        let written = on_disk(&dir);
        assert_eq!(written["attention"], "wait");
        assert_eq!(written["lines"][0], "hello");
    }

    #[test]
    fn a_word_outside_its_vocabulary_is_dropped_and_named() {
        let mut pane = Pane::at(scratch("lossy"));

        let answered = pane.publish("flow", &json!({"lines": [{"text": "x", "mark": "doing"}]}));

        let Answered::Lossy(sentence) = answered else { panic!("{answered:?}") };
        assert!(sentence.contains("doing"), "{sentence}");
    }

    #[test]
    fn a_document_without_lines_is_refused_and_writes_nothing() {
        let dir = scratch("refused");
        let mut pane = Pane::at(dir.clone());

        assert!(matches!(pane.publish("flow", &json!({"phase": "x"})), Answered::Failed(_)));
        assert!(!dir.join("panel.json").exists());
    }

    #[test]
    fn a_patch_edits_the_published_document_and_rewrites_the_file() {
        let dir = scratch("patch");
        let mut pane = Pane::at(dir.clone());
        pane.publish("flow", &json!({"lines": [{"key": "now", "text": "build"}]}));

        let answered = pane.patch(&json!([
            {"op": "replace", "key": "now", "line": {"key": "now", "text": "review"}},
            {"op": "set", "field": "attention", "value": "done"}
        ]));

        assert_eq!(answered, Answered::Ok);
        let written = on_disk(&dir);
        assert_eq!(written["lines"][0]["text"], "review");
        assert_eq!(written["attention"], "done");
    }

    #[test]
    fn a_patch_that_names_no_line_changes_nothing() {
        let dir = scratch("patch-refused");
        let mut pane = Pane::at(dir.clone());
        pane.publish("flow", &json!({"lines": [{"key": "now", "text": "build"}]}));
        let before = on_disk(&dir);

        let answered = pane.patch(&json!([
            {"op": "set", "field": "phase", "value": "ship"},
            {"op": "delete", "key": "tets"}
        ]));

        assert!(matches!(answered, Answered::Failed(ref why) if why.contains("tets")), "{answered:?}");
        assert_eq!(on_disk(&dir), before, "a refused patch rewrote the panel");
    }

    #[test]
    fn a_patch_before_any_publication_says_which_case_it_is() {
        let dir = scratch("unclaimed");
        let mut pane = Pane::at(dir.clone());
        assert_eq!(
            pane.patch(&json!([{"op": "set", "field": "phase", "value": "x"}])),
            Answered::Failed(NOTHING_YET.to_owned())
        );

        // Somebody else's file is there, but this process never claimed the pane.
        Pane::at(dir.clone()).publish("ci", &json!({"lines": ["from the CI"]}));
        let answered = pane.patch(&json!([{"op": "set", "field": "phase", "value": "x"}]));
        assert!(matches!(answered, Answered::Failed(ref why) if why.contains("publish with panel_set")), "{answered:?}");
    }

    #[test]
    fn a_new_session_reads_back_what_the_last_one_left() {
        let dir = scratch("resume");
        Pane::at(dir.clone()).publish("flow", &json!({"phase": "review", "lines": ["left here"]}));

        let answered = Pane::at(dir).read_the_document();

        let Answered::Document(text) = answered else { panic!("{answered:?}") };
        let document: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        assert_eq!(document["phase"], "review");
        assert_eq!(document["lines"][0], "left here");
    }

    #[test]
    fn retiring_removes_the_panel_and_only_its_publisher_may() {
        let dir = scratch("retire");
        let mut publisher = Pane::at(dir.clone());
        publisher.publish("flow", &json!({"lines": ["x"]}));

        assert!(matches!(Pane::at(dir.clone()).retire(), Answered::Failed(_)));
        assert!(dir.join("panel.json").exists());

        assert_eq!(publisher.retire(), Answered::Retired);
        assert!(!dir.join("panel.json").exists());
    }

    fn calling(attention: Option<Attention>, since: Option<u64>) -> Panel {
        let mut panel = panel_contract::parse_document(r#"{"lines":[]}"#).expect("a panel");
        panel.attention = attention;
        panel.attention_since = since;
        panel
    }

    #[test]
    fn the_same_call_keeps_the_moment_it_started() {
        let before = calling(Some(Attention::Wait), Some(100));
        let next = stamped(Some(&before), calling(Some(Attention::Wait), None), 500);
        assert_eq!(next.attention_since, Some(100));
    }

    #[test]
    fn a_different_call_starts_the_clock_again() {
        let before = calling(Some(Attention::Block), Some(100));
        let next = stamped(Some(&before), calling(Some(Attention::Wait), None), 500);
        assert_eq!(next.attention_since, Some(500));

        let first = stamped(None, calling(Some(Attention::Wait), None), 700);
        assert_eq!(first.attention_since, Some(700));
    }

    #[test]
    fn no_call_no_moment_and_the_agents_own_value_is_replaced() {
        let before = calling(Some(Attention::Wait), Some(100));
        let quiet = stamped(Some(&before), calling(None, Some(42)), 500);
        assert_eq!(quiet.attention_since, None);

        let claimed = stamped(None, calling(Some(Attention::Done), Some(42)), 500);
        assert_eq!(claimed.attention_since, Some(500));
    }

    /// A file left by an older server carries the call but not its moment: the clock starts now
    /// rather than staying empty, or that console would never be ordered.
    #[test]
    fn a_call_without_its_moment_gets_one() {
        let before = calling(Some(Attention::Wait), None);
        let next = stamped(Some(&before), calling(Some(Attention::Wait), None), 500);
        assert_eq!(next.attention_since, Some(500));
    }

    /// End to end through the file: a new session publishing the same wait keeps the old moment.
    #[test]
    fn a_new_session_publishing_the_same_wait_keeps_its_age() {
        let dir = scratch("since");
        std::fs::create_dir_all(&dir).expect("the directory");
        std::fs::write(dir.join("panel.json"), r#"{"attention":"wait","attention_since":100,"lines":[]}"#)
            .expect("an old panel");

        Pane::at(dir.clone()).publish("flow", &json!({"attention": "wait", "lines": ["again"]}));
        assert_eq!(on_disk(&dir)["attention_since"], 100);

        let mut pane = Pane::at(dir.clone());
        pane.publish("flow", &json!({"attention": "wait", "lines": ["again"]}));
        pane.patch(&json!([{"op": "set", "field": "attention", "value": "done"}]));
        assert_ne!(on_disk(&dir)["attention_since"], 100, "a new call kept the old moment");
        pane.patch(&json!([{"op": "set", "field": "attention", "value": null}]));
        assert!(on_disk(&dir).get("attention_since").is_none());
    }

    #[test]
    fn only_a_plain_terminal_id_names_a_directory() {
        assert_eq!(terminal_id("t9"), Ok("t9"));
        assert!(terminal_id("").is_err());
        assert!(terminal_id("../t9").is_err());
        assert!(terminal_id("vps/t3").is_err());
    }
}
