//! The MCP server an agent launches inside a Herdr console to publish that console's status panel.
//!
//! It speaks the same four panel verbs as Ivy's `panel-mcp` — `panel_set`, `panel_get`,
//! `panel_patch` and `panel_retire`, with the same names, arguments and reply words — so a producer
//! written for Ivy (the `flow` plugin) publishes here unchanged. What differs is the other side: Ivy
//! hands the document to its window over a socket; Herdr has no such window process, so this server
//! keeps the document and writes it where the Bellwether mods read it (see `pane.rs`).
//!
//! Ivy's three `split_*` verbs are not served yet, so they are not advertised.
//!
//! Which console it publishes for is not a parameter: it is the `HC_TERMINAL_ID` this process
//! inherited from the agent that launched it.

mod pane;
mod project;
mod status;

use std::io::{BufRead, Read, Write};

use panel_contract::{Attention, MAX_BYTES, MAX_SUMMARY, Mark, Style};
use pane::{Answered, Pane};
use project::Project;

const PROTOCOL_VERSION: &str = "2025-06-18";

/// The longest request line read: a whole document at its ceiling plus the envelope around it.
const MAX_REQUEST: usize = 2 * MAX_BYTES as usize;

fn main() {
    // Opened on the first call that needs it: listing the tools must work anywhere.
    let mut pane: Option<Pane> = None;
    bridge(std::io::stdin().lock(), std::io::stdout(), &mut pane);
}

/// Answers messages until the input ends. Bytes are decoded per line, so one malformed message is
/// the only casualty; a line over the ceiling is answered and its tail swallowed.
fn bridge(mut input: impl BufRead, mut output: impl Write, pane: &mut Option<Pane>) {
    let mut line = Vec::new();
    loop {
        line.clear();
        match input.by_ref().take(MAX_REQUEST as u64 + 1).read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(failure) => {
                eprintln!("bellwether-panel-mcp: reading a message failed ({failure})");
                break;
            }
        }
        let ended = line.last() == Some(&b'\n');
        if line.len() - usize::from(ended) > MAX_REQUEST {
            let answered = failed(&serde_json::Value::Null, -32600, "the message goes over the ceiling");
            if writeln!(output, "{answered}").and_then(|()| output.flush()).is_err() {
                break;
            }
            if !ended && swallow_to_newline(&mut input).is_err() {
                break;
            }
            continue;
        }
        let answered = match std::str::from_utf8(&line) {
            Ok(text) if text.trim().is_empty() => continue,
            Ok(text) => answer(text, pane),
            Err(failure) => Some(failed(
                &serde_json::Value::Null,
                -32700,
                &format!("the message is not UTF-8 ({failure})"),
            )),
        };
        let Some(answered) = answered else { continue };
        if writeln!(output, "{answered}").is_err() || output.flush().is_err() {
            break;
        }
    }
}

fn swallow_to_newline(input: &mut impl BufRead) -> std::io::Result<()> {
    let mut scratch = Vec::new();
    loop {
        scratch.clear();
        let read = input.by_ref().take(MAX_REQUEST as u64 + 1).read_until(b'\n', &mut scratch)?;
        if read == 0 || scratch.last() == Some(&b'\n') {
            return Ok(());
        }
    }
}

/// Answers one message, or nothing when it is a notification.
fn answer(line: &str, pane: &mut Option<Pane>) -> Option<String> {
    let message: serde_json::Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(failure) => return Some(failed(&serde_json::Value::Null, -32700, &failure.to_string())),
    };
    let id = message.get("id")?.clone();
    let Some(method) = message.get("method").and_then(serde_json::Value::as_str) else {
        return Some(failed(&id, -32600, "the message carries no method"));
    };
    Some(match method {
        "initialize" => succeeded(&id, initialized(&message)),
        "tools/list" => succeeded(&id, tools()),
        "tools/call" => called(&id, &message, pane),
        "ping" => succeeded(&id, serde_json::json!({})),
        other => failed(&id, -32601, &format!("no method \"{other}\"")),
    })
}

fn initialized(message: &serde_json::Value) -> serde_json::Value {
    let asked_for = message
        .pointer("/params/protocolVersion")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    serde_json::json!({
        "protocolVersion": asked_for,
        "capabilities": {"tools": {}},
        "serverInfo": {"name": "bellwether panel", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// `a, b or c`, written from the vocabulary's own list.
fn listed(words: &[&str]) -> String {
    match words.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, init)) => format!("{} or {last}", init.join(", ")),
        None => String::new(),
    }
}

fn tools() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "panel_set",
                "description": format!(
                    "Publish this console's status panel. The document is the panel document: a \
                     required \"lines\" array, and optionally \"phase\", \"attention\", \
                     \"actions\" and the rest of the contract. A line is a string, or an object \
                     with \"text\" and optionally \"mark\", \"style\", \"indent\", \"key\", \
                     \"ref\", \"link\" and \"summary\". A line with style \"section\" is a block's \
                     header: its text on the left, a rule, and its \"summary\" (at most {} \
                     characters, e.g. \"2/4 merged\") pinned to the right. Three fields take a \
                     closed vocabulary: \"attention\" \
                     is {}; a line's \"mark\" is {}; its \"style\" is {}. Any other non-null \
                     value is dropped, the panel still paints, and the reply names what fell away. \
                     A section header with \"collapsed\": true starts folded, showing only its \
                     title and summary: use it for reference blocks, such as merged MRs. \
                     \"milestone\" and \"deliverable\" name the ids of the project plan \
                     (plan_set) this console works on. \"attention_since\" is kept by this server: \
                     it records when the current attention started. Publishing makes this process \
                     the console's producer until it exits; panel_patch needs it.",
                    MAX_SUMMARY,
                    listed(&Attention::ALL.map(Attention::name)),
                    listed(&Mark::ALL.map(Mark::name)),
                    listed(&Style::ALL.map(Style::name)),
                ),
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source": {"type": "string", "description": "What to call whoever is publishing."},
                        "document": {"type": "object", "description": "The panel document."},
                    },
                    "required": ["source", "document"],
                },
            },
            {
                "name": "panel_get",
                "description": "Read back the document this console's status panel is showing, \
                                including one a previous session left behind. It takes no \
                                arguments, claims nothing and changes nothing: it is how a process \
                                that restarted finds out what is on screen before publishing again.",
                "inputSchema": {"type": "object", "properties": {}},
            },
            {
                "name": "panel_retire",
                "description": "Intentionally end the session currently driving this console's \
                                panel: remove its panel. Only the session that published it may.",
                "inputSchema": {"type": "object", "properties": {}},
            },
            {
                "name": "panel_patch",
                "description": "Change part of this console's status panel without republishing \
                                the rest of it. Each op names what it does: \"replace\", \"before\", \
                                \"after\" and \"delete\" address a line by the \"key\" its producer \
                                gave it, \"first\" and \"last\" put a line at either end of the \
                                body, and \"set\" changes one of \"phase\", \"attention\", \
                                \"header\" or \"stale_after_minutes\". Either the whole list \
                                applies or none of it does: a key that names no line, or two, \
                                refuses the patch and leaves the panel exactly as it was. It needs \
                                this console claimed with panel_set first, and buttons are not \
                                patchable. A line an op carries is read like a line of panel_set's \
                                document: a mark or style outside its vocabulary is dropped and \
                                named in the reply.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ops": {
                            "type": "array",
                            "description": "The edits, applied in order; each one sees what the ones before it did.",
                            "items": {"type": "object"},
                        },
                    },
                    "required": ["ops"],
                },
            },
            {
                "name": "plan_set",
                "description": "Publish the roadmap of the project this console works in: one plan per \
                                git repository, shared by its worktrees, painted on the board under \
                                the project. The document has a required \"milestones\" array and \
                                optionally \"title\" (what the plan delivers) and \"source\" (where it \
                                was read from, e.g. docs/roadmap.md). A milestone is an object with \
                                a required \"id\" and optionally \"title\", \"done_when\", \
                                \"depends_on\" (milestone ids) and \"items\". An item is a string, or \
                                an object with \"text\" and optionally \"id\", \"mark\" (done, \
                                current, pending, wait or block), \"ref\" (its ticket) and \"link\". \
                                \"outside\" lists non-code work the plan depends on, as items. \
                                Copy each state as the source states it; the board only counts what \
                                is marked done. A console says where it works with its panel's \
                                \"milestone\" and \"deliverable\".",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "document": {"type": "object", "description": "The plan document."},
                        "repo": {"type": "string", "description": "Absolute path inside the repository; this process's working directory when absent."},
                    },
                    "required": ["document"],
                },
            },
            {
                "name": "plan_get",
                "description": "Read back the roadmap published for this project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"repo": {"type": "string", "description": "As in plan_set."}},
                },
            },
            {
                "name": "plan_retire",
                "description": "Remove this project's published roadmap from the board.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"repo": {"type": "string", "description": "As in plan_set."}},
                },
            },
        ],
    })
}

/// What one tool call asks for, read and validated before the console is reached.
enum Ask {
    Publish(String, serde_json::Value),
    ReadTheDocument,
    Retire,
    Patch(serde_json::Value),
}

/// The word an empty `ok` answers with, decided from what was asked.
fn what_it_did(ask: &Ask) -> &'static str {
    match ask {
        Ask::Patch(_) => "patched",
        Ask::Retire => "retired",
        Ask::Publish(..) | Ask::ReadTheDocument => "published",
    }
}

fn called(id: &serde_json::Value, message: &serde_json::Value, pane: &mut Option<Pane>) -> String {
    let params = message.get("params").unwrap_or(&serde_json::Value::Null);
    let name = params.get("name").and_then(serde_json::Value::as_str).unwrap_or_default();
    let arguments = params.get("arguments").unwrap_or(&serde_json::Value::Null);
    let ask = match name {
        "panel_set" => {
            let Some(source) = arguments.get("source").and_then(serde_json::Value::as_str) else {
                return succeeded(id, refusal("panel_set needs a \"source\""));
            };
            let Some(document) = arguments.get("document") else {
                return succeeded(id, refusal("panel_set needs a \"document\""));
            };
            Ask::Publish(source.to_owned(), document.clone())
        }
        "panel_get" => Ask::ReadTheDocument,
        "panel_retire" => Ask::Retire,
        "panel_patch" => {
            let Some(ops) = arguments.get("ops").filter(|ops| ops.is_array()) else {
                return succeeded(id, refusal("panel_patch needs an \"ops\" list"));
            };
            Ask::Patch(ops.clone())
        }
        "plan_set" | "plan_get" | "plan_retire" => return succeeded(id, planned(name, arguments)),
        _ => return failed(id, -32602, &format!("no tool \"{name}\"")),
    };

    // A console that cannot be reached is a tool error the agent reads and carries on from.
    let pane = match pane {
        Some(pane) => pane,
        None => match Pane::of_this_console() {
            Ok(opened) => pane.insert(opened),
            Err(why) => return succeeded(id, refusal(&why)),
        },
    };
    let did = what_it_did(&ask);
    let answered = match ask {
        Ask::Publish(source, document) => pane.publish(&source, &document),
        Ask::ReadTheDocument => pane.read_the_document(),
        Ask::Retire => pane.retire(),
        Ask::Patch(ops) => pane.patch(&ops),
    };
    succeeded(id, result_of(did, answered))
}

/// The plan verbs: they need a repository, not a console, so they never open the pane.
fn planned(name: &str, arguments: &serde_json::Value) -> serde_json::Value {
    let repo = arguments.get("repo").and_then(serde_json::Value::as_str);
    let project = match Project::of(repo) {
        Ok(project) => project,
        Err(why) => return refusal(&why),
    };
    match name {
        "plan_set" => match arguments.get("document") {
            Some(document) => result_of("published", project.publish(document)),
            None => refusal("plan_set needs a \"document\""),
        },
        "plan_get" => result_of("published", project.read()),
        _ => match project.retire() {
            Answered::Retired => text("retired"),
            other => result_of("retired", other),
        },
    }
}

fn result_of(did: &str, answered: Answered) -> serde_json::Value {
    match answered {
        Answered::Ok => text(did),
        Answered::Lossy(dropped) => text(&format!("{did}; {dropped}")),
        // Ivy's literal reply: this server never owns auxiliary consoles, so both counts are zero.
        Answered::Retired => text("retired; closed 0 auxiliary console(s); preserved 0 taken-over console(s)"),
        Answered::Document(document) => text(&document),
        Answered::Failed(why) => refusal(&why),
    }
}

fn text(what: &str) -> serde_json::Value {
    serde_json::json!({"content": [{"type": "text", "text": what}]})
}

fn refusal(why: &str) -> serde_json::Value {
    serde_json::json!({"content": [{"type": "text", "text": why}], "isError": true})
}

fn succeeded(id: &serde_json::Value, result: serde_json::Value) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn failed(id: &serde_json::Value, code: i32, message: &str) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(message: &str) -> serde_json::Value {
        let answered = answer(message, &mut None).expect("it deserved an answer");
        serde_json::from_str(&answered).expect("the answer is JSON")
    }

    #[test]
    fn the_four_panel_verbs_are_advertised_under_ivys_names() {
        let answered = ask(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        let names: Vec<&str> = answered["result"]["tools"]
            .as_array()
            .expect("a list of tools")
            .iter()
            .map(|tool| tool["name"].as_str().expect("named"))
            .collect();
        assert_eq!(
            names,
            ["panel_set", "panel_get", "panel_retire", "panel_patch", "plan_set", "plan_get", "plan_retire"]
        );
    }

    /// **The description is the only copy of the contract an agent reads**, so the header style has
    /// to be announced under the style's own clause — the vocabulary list is written from
    /// `Style::ALL` — and its `summary` field named, or a producer never learns either exists.
    #[test]
    fn the_publishing_description_announces_the_block_header_and_its_summary() {
        let answered = ask(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        let publishing = answered["result"]["tools"][0]["description"].as_str().expect("described").to_owned();

        assert!(
            publishing.contains("its \"style\" is normal, dim, title, accent, ok, warn, error or section."),
            "{publishing}"
        );
        assert!(publishing.contains("\"summary\""), "{publishing}");
    }

    #[test]
    fn the_publishing_description_announces_folded_blocks_and_the_plan_link() {
        let answered = ask(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#);
        let publishing = answered["result"]["tools"][0]["description"].as_str().expect("described").to_owned();
        for field in ["\"collapsed\"", "\"milestone\"", "\"deliverable\"", "\"attention_since\""] {
            assert!(publishing.contains(field), "{field} is not announced: {publishing}");
        }
    }

    #[test]
    fn a_plan_verb_without_a_repository_is_refused_not_crashed() {
        let answered = ask(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"plan_get","arguments":{"repo":"relative/path"}}}"#);
        assert_eq!(answered["result"]["isError"], true, "{answered}");
    }

    #[test]
    fn a_notification_is_not_answered() {
        assert!(answer(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, &mut None).is_none());
    }

    #[test]
    fn a_tool_that_was_never_advertised_is_a_protocol_error() {
        let answered = ask(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"split_open","arguments":{}}}"#);
        assert_eq!(answered["error"]["code"], -32602);
    }

    #[test]
    fn a_publication_answers_published_and_a_patch_patched() {
        let dir = std::env::temp_dir().join(format!("bellwether-panel-mcp-{}-bridge", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut pane = Some(Pane::at(dir));
        let call = |pane: &mut Option<Pane>, body: &str| -> serde_json::Value {
            let line = format!(r#"{{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{body}}}"#);
            serde_json::from_str(&answer(&line, pane).expect("answered")).expect("JSON")
        };

        let set = call(&mut pane, r#"{"name":"panel_set","arguments":{"source":"flow","document":{"lines":[{"key":"now","text":"a"}]}}}"#);
        assert_eq!(set["result"]["content"][0]["text"], "published", "{set}");
        let patched = call(&mut pane, r#"{"name":"panel_patch","arguments":{"ops":[{"op":"delete","key":"now"}]}}"#);
        assert_eq!(patched["result"]["content"][0]["text"], "patched", "{patched}");
    }

    #[test]
    fn a_message_that_is_not_utf8_is_answered_and_the_next_one_still_is() {
        let mut input: Vec<u8> = Vec::new();
        input.extend_from_slice(&[0xff, b'\n']);
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"ping\"}\n");
        let mut written = Vec::new();

        bridge(&input[..], &mut written, &mut None);

        let answers: Vec<serde_json::Value> = String::from_utf8(written)
            .expect("text")
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSON"))
            .collect();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[1]["id"], 5);
    }
}
