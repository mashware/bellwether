//! Herdr's own agent status (OSC 7501), written for a console whose program does not report one.
//!
//! Claude Code and Codex both report their status to Herdr themselves — working, done, and blocked
//! on a question or a permission — so on their consoles this module writes nothing: one console has
//! one status in Herdr and the last writer wins, and the two would overwrite each other (measured:
//! Codex's own `done` at the end of its turn replaced a `blocked` written from here). For an agent
//! that reports nothing, Herdr's agents list, sounds and notifications learn about the console only
//! from what the producer publishes here.
//!
//! The sequence has to come out of the console itself, so it is written to the controlling terminal
//! this process inherited from its agent. One short `write`: the terminal layer does not interleave
//! it with the agent's own writes, and an OSC draws nothing.
//!
//! Format, as Herdr reads it and as Claude Code writes it: `ESC ] 7501 ; state=…:app=…[:kind=…]
//! [:msg=<base64>] ESC \`, and `state=clear` to drop the status.

use std::io::Write;

use panel_contract::{Attention, Mark, Panel};

/// The longest message carried, in characters: Herdr shows it in one row.
const MAX_MSG: usize = 200;

/// Decides once whether this process may write the console's status, then writes it.
#[derive(Debug)]
pub struct Sink {
    terminal: String,
    app: String,
    /// `None` until the first write asks Herdr whether somebody else already reports here.
    owned: Option<bool>,
}

impl Sink {
    pub fn new(terminal: &str) -> Self {
        Self { terminal: terminal.to_owned(), app: parent_name(), owned: None }
    }

    /// The status for `panel`, or a clear when there is no panel any more. Failures are silent on
    /// purpose: the panel was published either way, and Herdr's status is a convenience on top.
    pub fn publish(&mut self, panel: Option<&Panel>) {
        let owned = *self.owned.get_or_insert_with(|| nobody_else_reports(&self.terminal));
        if !owned {
            return;
        }
        let sequence = format!("\x1b]7501;{}\x1b\\", record(panel, &self.app));
        if let Ok(mut tty) = std::fs::OpenOptions::new().write(true).open("/dev/tty") {
            let _ = tty.write_all(sequence.as_bytes());
        }
    }
}

/// The record for `panel`: what `attention` says, in Herdr's words. `block` has no word of its own
/// in Herdr (its `blocked` means a person is needed), so it stays `working` with why it is stuck.
pub fn record(panel: Option<&Panel>, app: &str) -> String {
    let Some(panel) = panel else { return "state=clear".to_owned() };
    let line = |mark: Mark| panel.lines.iter().find(|line| line.mark == Some(mark)).map(|line| line.text.as_str());
    let phase = panel.phase.as_deref();
    let (state, kind, msg) = match panel.attention {
        Some(Attention::Wait) => ("blocked", Some("question"), line(Mark::Wait).or(phase).map(str::to_owned)),
        Some(Attention::Done) => ("done", None, phase.map(str::to_owned)),
        Some(Attention::Block) => ("working", None, line(Mark::Block).map(|why| format!("stuck: {why}"))),
        None => ("working", None, phase.map(str::to_owned)),
    };
    let mut fields = vec![format!("state={state}"), format!("app={app}")];
    if let Some(kind) = kind {
        fields.push(format!("kind={kind}"));
    }
    if let Some(msg) = msg.filter(|msg| !msg.trim().is_empty()) {
        let clean: String = msg.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_MSG).collect();
        fields.push(format!("msg={}", base64(clean.as_bytes())));
    }
    fields.join(":")
}

/// Whether this console's status is free for us: nobody has reported on it. A program that
/// reports for itself always has by the time it publishes a panel — Claude Code and Codex both
/// report `working` as their turn starts — and Herdr clears the status when the agent exits, so a
/// later session starts free again. The app name proves nothing: Codex reports as `codex`, which is
/// also what this server would call it. Anything that cannot be asked counts as taken.
fn nobody_else_reports(terminal: &str) -> bool {
    let hc = ["hc", "/usr/local/bin/hc"]
        .into_iter()
        .map(str::to_owned)
        .chain(std::env::var_os("HOME").map(|home| format!("{}/.local/bin/hc", home.to_string_lossy())));
    for program in hc {
        let Ok(out) = std::process::Command::new(&program).args(["get", terminal, "--json"]).output() else {
            continue;
        };
        if !out.status.success() {
            return false;
        }
        let Ok(got) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return false };
        return matches!(got.get("status"), None | Some(serde_json::Value::Null));
    }
    false
}

/// The name of the program that launched this server — the agent (`codex`) — as Herdr's `app`.
fn parent_name() -> String {
    let parent = std::os::unix::process::parent_id();
    let name = std::fs::read_to_string(format!("/proc/{parent}/comm")).unwrap_or_default();
    let name: String = name.trim().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if name.is_empty() { "bellwether".to_owned() } else { name }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { TABLE[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use panel_contract::parse_document;

    fn panel(json: &str) -> Panel {
        parse_document(json).expect("a panel")
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b"answer: Approve the design?"), "YW5zd2VyOiBBcHByb3ZlIHRoZSBkZXNpZ24/");
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn waiting_for_you_is_herdrs_blocked_question_with_the_question_as_message() {
        let record = record(Some(&panel(r#"{"attention":"wait","phase":"design","lines":[{"text":"Approve the design?","mark":"wait"}]}"#)), "codex");
        assert_eq!(record, format!("state=blocked:app=codex:kind=question:msg={}", base64(b"Approve the design?")));
    }

    #[test]
    fn stuck_on_something_else_is_not_herdrs_blocked() {
        let record = record(Some(&panel(r#"{"attention":"block","lines":[{"text":"the CI is red","mark":"block"}]}"#)), "codex");
        assert!(record.starts_with("state=working:app=codex"), "{record}");
        assert!(record.ends_with(&base64(b"stuck: the CI is red")), "{record}");
    }

    #[test]
    fn finished_is_done_and_no_claim_is_working_and_no_panel_clears() {
        assert!(record(Some(&panel(r#"{"attention":"done","lines":["x"]}"#)), "codex").starts_with("state=done:"));
        assert!(record(Some(&panel(r#"{"lines":["x"]}"#)), "codex").starts_with("state=working:"));
        assert_eq!(record(None, "codex"), "state=clear");
    }

    #[test]
    fn a_message_carries_no_control_character_and_is_bounded() {
        let msg = |phase: &str| {
            let record = record(Some(&panel(&format!(r#"{{"attention":"wait","phase":"{phase}","lines":["x"]}}"#))), "codex");
            record.rsplit("msg=").next().expect("a message").to_owned()
        };
        assert_eq!(msg("a\\u001b]b"), base64(b"a ]b"));
        assert_eq!(msg(&"a".repeat(400)), base64("a".repeat(MAX_MSG).as_bytes()));
    }
}
