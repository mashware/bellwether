//! Changing one line of a document without republishing the rest.
//!
//! **The vocabulary is the document's and not the transport's**, which is why it lives in this crate
//! beside [`Panel`] and not in `panel-control`. The day a second transport arrives it speaks these
//! same ops through this same reader, and *both transports accept the same document* stays literally
//! true rather than nominally (**D17**).
//!
//! **It sits in its own module rather than in `panel.rs`** for the reason the size rule gives
//! (**D28**): what a document *is* and how it is read is one subject, and how one is edited in place
//! is another. Nothing about the placement changes what the design declared — the ops are this
//! crate's, reached through its own re-exports.
//!
//! **Everything decidable without the document is decided in [`parse_ops`]**, on the connection's
//! own thread and outside the frame: an unknown op name, a `set` naming a field that is not
//! patchable, an `attention` value that is not one of the three, a `line` that is neither a string
//! nor an object, more ops than the ceiling allows. **What is left for [`Panel::patched_with`] is
//! the one question that needs the document** — does this key name a line here.
//!
//! **What that costs in the frame is more than that question, and saying otherwise was the first
//! version of this paragraph.** Applying a patch clones the panel, builds the key index over it, and
//! — at the caller, because the ceiling is the caller's — writes the result out to weigh it. All
//! three scale with the whole document and not with the patch, so a one-op patch on a panel at the
//! ceiling costs about what publishing that panel costs. That is the price of the verb, not a
//! defect: what it saves is the *bytes on the wire* and the producer's own tokens, which is what it
//! was asked for.
//!
//! **D47.**

use std::time::Duration;

use super::dropped::Dropped;
use super::panel::{self, Attention, Line, Panel, Problem, STALE_AFTER};

/// The most ops one request may carry.
///
/// **It bounds the work this verb puts inside a frame, not the bytes**, which the transport's own
/// ceiling already holds. Without it, sixty-four thousand ops over a document of tens of thousands
/// of lines is unbounded work in one frame; with it, the worst case is bounded and measurable.
///
/// **What that worst case actually is, corrected in review.** The keys are resolved in **one pass** —
/// the index is built once, O(lines) — but maintaining it is not free: an insert or a delete walks
/// every position the index holds, so a patch is O(lines + ops × keyed lines) and not the
/// O(lines + ops) an earlier version of this paragraph claimed. At the document ceiling that is of
/// the same order as the figure the number was chosen against, so the ceiling still does its job —
/// but the two are not the same statement, and the one that was written was false.
///
/// **Surplus ops are refused and never truncated**, which is the opposite of what `actions` does and
/// deliberately so: a truncated button is a button nobody sees, and a truncated instruction is an
/// edit that silently did not happen with the producer told `{"ok":true}` — the exact failure this
/// verb exists to remove.
pub const MAX_OPS: usize = 64;

/// One edit to a published document.
///
/// **The ops within one request see each other's effects**, applied in order to a working copy: an
/// `After` may name a key an earlier op inserted, and deleting an already-deleted key refuses. The
/// alternative — every op resolved against the document as it arrived — makes two `After` ops on one
/// key ambiguous about their final order and makes *insert a line, then replace it* unexpressible.
/// A list of edits is read the way a person writes it, and sequential visibility costs nothing in
/// atomicity because the copy is one nobody is painting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// The line carrying this key becomes this one.
    Replace { key: String, line: Line },
    /// This line goes immediately before the one carrying this key.
    Before { key: String, line: Line },
    /// Immediately after it.
    After { key: String, line: Line },
    /// At the start of the body.
    First { line: Line },
    /// At the end of the body.
    Last { line: Line },
    /// The line carrying this key goes.
    Delete { key: String },
    /// One scalar field of the document, carrying what it is being set to.
    Set(Scalar),
}

/// A scalar of the document, **carrying what it is being set to**.
///
/// **One enum and not a field beside a value, which is what the first shape had.** Two parallel
/// enums are two things a reader has to keep in step and a type that cannot: a `Header` paired with
/// a phase string was constructible, and the applying code — obliged to answer for a pair that
/// cannot arrive from the parser — absorbed it into «as if unsaid», silently clearing a field nobody
/// had named. Here the pair does not exist, and applying one is four total arms.
///
/// **The `Option` inside is the JSON null, and it is already resolved.** Null means for every field
/// what the document not having said it means, and *that* differs per field — absence for `phase`
/// and `attention`, the default for `header` and `stale_after_minutes`, because absence is what
/// produces a default. Resolving it where the field is known is what keeps the difference out of the
/// applying code.
///
/// **`actions` is not here, and its absence is the ban.** A button reaches a shell, so it is only
/// ever set with the whole document — which is scope rather than security, and argued as such: a
/// holder already sets any buttons it likes through a publication, so a patch would neither bypass a
/// gate nor add reach. What it would add is a second path into the one shape of this contract that
/// runs a command, for a convenience nobody asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scalar {
    Phase(Option<String>),
    Attention(Option<Attention>),
    Header(bool),
    StaleAfter(Duration),
}

/// Reads a request's `ops` array, refusing everything a document is not needed to refuse.
///
/// **The tolerance rule is *what the op says* against *what the op carries*, and the two halves are
/// read differently on purpose.** A `line` an op carries **is a document line** and goes through the
/// document's own reader, so a mark nobody knows is dropped and the line stands. A **field named by a
/// `set` is part of the instruction** and is not tolerated: `"attention":"waiting"` refuses the whole
/// patch, where the same word inside a published document is dropped and the panel still paints.
///
/// That divergence is the sharpest point of this design and it is deliberate. A document is somebody's
/// summary and losing it over a field nobody looks at would be the worst thing to do with it; a
/// request is an instruction, and an instruction nobody can read has no sensible reading at all. **The
/// document did not change; what can be said about it diverges.**
pub fn parse_ops(raw: &serde_json::Value) -> Result<Patch, Problem> {
    read_ops(raw, None)
}

/// The same reading as [`parse_ops`], and **also what the lines the ops carry let fall** — a mark or
/// a style read as nothing, which a document line tolerates and the channel's reply names (**D58**).
///
/// The patch it returns is the one [`parse_ops`] returns for the same ops, always. A refused request
/// reports nothing about its lines: the refusal is the whole answer.
pub fn parse_ops_noting(raw: &serde_json::Value) -> Result<(Patch, Dropped), Problem> {
    let mut dropped = Dropped::default();
    let patch = read_ops(raw, Some(&mut dropped))?;
    Ok((patch, dropped))
}

/// The one reader behind both doors, recording in `dropped` when there is somewhere to record.
fn read_ops(raw: &serde_json::Value, mut dropped: Option<&mut Dropped>) -> Result<Patch, Problem> {
    let asked = raw
        .as_array()
        .ok_or_else(|| refused("\"ops\" is not a list"))?;
    // **An empty list is refused rather than applied.** Applying none of no ops succeeds by
    // definition, and the pane would answer `{"ok":true}`, count a patch and stamp the panel's date
    // — so a producer looping on empty patches would keep a panel looking fresh without ever having
    // published anything, which is precisely the silence the stale notice exists to break.
    if asked.is_empty() {
        return Err(refused("a patch carries no ops"));
    }
    if asked.len() > MAX_OPS {
        return Err(refused(&format!(
            "a patch carries at most {MAX_OPS} ops and this one carries {}",
            asked.len()
        )));
    }

    let ops = asked
        .iter()
        .map(|op| one_op(op, dropped.as_deref_mut()))
        .collect::<Result<_, _>>()?;
    Ok(Patch(ops))
}

/// A list of ops **that is already under [`MAX_OPS`]**.
///
/// **It exists so the ceiling travels with the ops instead of staying behind in the reader.** The
/// bound was checked in [`parse_ops`] and nowhere else, while [`Panel::patched_with`] — public, and
/// re-exported — took any slice at all: the one door that enforced it was on the far side of this
/// crate's public boundary. The second transport **D17** promises would have walked in past it,
/// which is exactly the worst case the number was reasoned against.
///
/// There is no constructor but [`parse_ops`] and [`parse_ops_noting`], which is the same reader, and
/// that is the whole of the type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch(Vec<Op>);

impl Patch {
    /// The ops, for whoever has to walk them.
    #[must_use]
    pub fn ops(&self) -> &[Op] {
        &self.0
    }

    /// How many there are, which is what a measurement asks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Never true — [`parse_ops`] refuses an empty list — and here because `len` without it is a
    /// lint, and because a caller reading `len() > 0` deserves the word.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One element of the list, or the reason it is not an op.
fn one_op(raw: &serde_json::Value, dropped: Option<&mut Dropped>) -> Result<Op, Problem> {
    let name = raw
        .get("op")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| refused("an op names no \"op\""))?;

    match name {
        "replace" => Ok(Op::Replace {
            key: key_of(raw, name)?,
            line: line_of(raw, name, dropped)?,
        }),
        "before" => Ok(Op::Before {
            key: key_of(raw, name)?,
            line: line_of(raw, name, dropped)?,
        }),
        "after" => Ok(Op::After {
            key: key_of(raw, name)?,
            line: line_of(raw, name, dropped)?,
        }),
        "first" => Ok(Op::First {
            line: line_of(raw, name, dropped)?,
        }),
        "last" => Ok(Op::Last {
            line: line_of(raw, name, dropped)?,
        }),
        "delete" => Ok(Op::Delete {
            key: key_of(raw, name)?,
        }),
        "set" => set_op(raw),
        other => Err(refused(&format!("no op is called \"{other}\""))),
    }
}

/// A `set`, with its field and its value read as one thing.
///
/// **The null is resolved here, where the field is known**, so nothing downstream has to remember
/// that an absent `phase` and a default `header` are both what null means.
fn set_op(raw: &serde_json::Value) -> Result<Op, Problem> {
    let named = raw
        .get("field")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| refused("a \"set\" names no \"field\""))?;
    // **`actions` is refused by its own name and not as an unknown field**, because the producer's
    // next move depends on which it is: a typo is corrected, and this one is a shape of the contract
    // that will not become patchable.
    if named == "actions" {
        return Err(refused(
            "\"actions\" is not patchable: a button reaches a shell, so it is only ever set with the whole document",
        ));
    }
    let asked = raw
        .get("value")
        .ok_or_else(|| refused("a \"set\" carries no \"value\""))?;
    let wrong_kind = || {
        refused(&format!(
            "the value a \"set\" gives \"{named}\" is not of that field's kind"
        ))
    };

    let scalar = match (named, asked) {
        ("phase", serde_json::Value::Null) => Scalar::Phase(None),
        ("phase", serde_json::Value::String(text)) => Scalar::Phase(Some(text.clone())),
        ("attention", serde_json::Value::Null) => Scalar::Attention(None),
        ("attention", serde_json::Value::String(name)) => {
            Scalar::Attention(Some(panel::attention_named(name).ok_or_else(|| {
                refused(&format!("no attention of a panel is called \"{name}\""))
            })?))
        }
        // The two whose absence produces a value rather than nothing: null lands on the default here,
        // so the applying code never has to know which kind of field it is holding.
        ("header", serde_json::Value::Null) => Scalar::Header(true),
        ("header", serde_json::Value::Bool(shown)) => Scalar::Header(*shown),
        ("stale_after_minutes", serde_json::Value::Null) => Scalar::StaleAfter(STALE_AFTER),
        ("stale_after_minutes", serde_json::Value::Number(minutes)) => {
            let minutes = minutes.as_u64().ok_or_else(wrong_kind)?;
            Scalar::StaleAfter(panel::expiry_of(minutes))
        }
        ("phase" | "attention" | "header" | "stale_after_minutes", _) => {
            return Err(wrong_kind());
        }
        // **A field of the document that is not patchable is not an unknown field**, and the two
        // sentences are different on purpose: a producer told its `updated_at` does not exist goes
        // hunting for a typo that is not there. `actions` gets a sentence of its own above because
        // its reason is its own; these are simply out of scope, and saying so is the whole answer.
        ("v" | "updated_at" | "lines" | "written_at" | "attention_since" | "milestone" | "deliverable", _) => {
            return Err(refused(&format!(
                "\"{named}\" is a field of a panel and not one a patch may set"
            )));
        }
        (other, _) => {
            return Err(refused(&format!(
                "no field of a panel is called \"{other}\""
            )));
        }
    };

    Ok(Op::Set(scalar))
}

/// The key an op addresses by, which the four addressing ops all require.
fn key_of(raw: &serde_json::Value, op: &str) -> Result<String, Problem> {
    raw.get("key")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| refused(&format!("a \"{op}\" names no \"key\"")))
}

/// The line an op carries, read by **the document's own reader** and nothing else.
fn line_of(
    raw: &serde_json::Value,
    op: &str,
    dropped: Option<&mut Dropped>,
) -> Result<Line, Problem> {
    let carried = raw
        .get("line")
        .ok_or_else(|| refused(&format!("a \"{op}\" carries no \"line\"")))?;
    // A value that is neither a string nor an object is not a line of anything — the same refusal
    // the document's own reader makes, reached through the same function so the two cannot drift.
    panel::read_line_noting(carried, dropped)
        .ok_or_else(|| refused(&format!("a \"{op}\" carries something that is not a line")))
}

/// This crate's own error, carrying the sentence the producer reads **and nothing around it**.
///
/// **[`Problem`] and not a second error type**, because `serialize` answers in this vocabulary and
/// the ceiling a patched document is weighed against is [`Problem::TooBig`]: a second error enum
/// inside one crate would make every caller translate between two ways of saying the same thing.
/// [`Problem::Refused`] and not [`Problem::Invalid`] because the wording travels — the contract
/// publishes these sentences, and «not understood (…)» wrapped around one is a second reading of a
/// request that was understood perfectly well.
fn refused(why: &str) -> Problem {
    Problem::Refused(why.to_owned())
}

impl Panel {
    /// A copy of this panel with the ops applied, or the reason none of them was.
    ///
    /// **All or nothing, and it returns a new panel rather than editing this one.** Three reasons,
    /// and the third is a precedent: a refusal has to leave the document byte-for-byte as it was,
    /// which cannot be true of a request whose other ops applied; the transport already draws the
    /// line between a summary tolerated per field and an instruction that has no partial reading;
    /// and **D15** transforms the pane tree *by value* and replaces it only if the transformation
    /// reaches the end, so no path leaves a live console outside the tree. Same shape here — the
    /// caller installs what comes back or drops it.
    ///
    /// **The keys are resolved in one pass and the index is maintained as the ops run.** Looking each
    /// key up by walking the body would be O(lines × ops); building the map once and repairing it is
    /// O(lines + ops × keyed lines), because an insert or a delete moves every position the index
    /// holds. Cheaper than the walk by the ratio of keyed lines to lines, which on a real panel is
    /// the few somebody meant to address — and bounded, which is what [`MAX_OPS`] is for.
    ///
    /// **It clones before it can refuse**, so a refused patch pays the clone and the index and skips
    /// only the write-out and the install. That is the price of all-or-nothing: an op list is only
    /// known to resolve by resolving it, because each op sees what the one before it did.
    ///
    /// **An ambiguous key refuses the op that names it.** Two lines may carry one key — reading
    /// somebody else's document is not the moment to rewrite it — so the ambiguity is left where the
    /// producer put it and reported to whoever tries to address it, which is when it matters and by
    /// the transport that can answer.
    ///
    /// **The ceiling is not checked here.** What the result weighs is `serialize`'s own question and
    /// it is asked by the caller, which is the party that also has to hand those bytes somewhere.
    pub fn patched_with(&self, patch: &Patch) -> Result<Self, Problem> {
        let mut patched = self.clone();
        let mut index = Index::of(&patched.lines);

        for op in patch.ops() {
            match op {
                Op::Replace { key, line } => {
                    let at = index.only(key)?;
                    // **The old address is given up before the new one is taken**, in that order and
                    // not the other way round: a line replaced by one carrying the same key must not
                    // collide with itself. An op later in the same request then addresses what this
                    // one left behind, not what it found.
                    index.forget(key);
                    index.remember(line.key.as_deref(), at)?;
                    patched.lines[at] = line.clone();
                }
                Op::Before { key, line } => {
                    let at = index.only(key)?;
                    index.inserted_at(at, line.key.as_deref())?;
                    patched.lines.insert(at, line.clone());
                }
                Op::After { key, line } => {
                    let at = index.only(key)? + 1;
                    index.inserted_at(at, line.key.as_deref())?;
                    patched.lines.insert(at, line.clone());
                }
                Op::First { line } => {
                    index.inserted_at(0, line.key.as_deref())?;
                    patched.lines.insert(0, line.clone());
                }
                Op::Last { line } => {
                    let at = patched.lines.len();
                    index.inserted_at(at, line.key.as_deref())?;
                    patched.lines.push(line.clone());
                }
                Op::Delete { key } => {
                    let at = index.only(key)?;
                    index.deleted_at(at);
                    patched.lines.remove(at);
                }
                Op::Set(scalar) => set(&mut patched, scalar),
            }
        }

        Ok(patched)
    }
}

/// Puts one scalar where it goes.
///
/// **Four arms and no catch-all**, which is what collapsing the field and its value bought: there is
/// no pair left to be impossible, so there is no branch here deciding what an impossible one means.
fn set(panel: &mut Panel, scalar: &Scalar) {
    match scalar {
        Scalar::Phase(text) => panel.phase = text.clone(),
        Scalar::Attention(calling) => panel.attention = *calling,
        Scalar::Header(shown) => panel.header = *shown,
        Scalar::StaleAfter(after) => panel.stale_after = *after,
    }
}

/// Where each key is, and whether it is anywhere twice.
///
/// **A key that names two lines is kept as two and not as one**, because the refusal it earns is the
/// whole point: dropping the duplicate here would hand the first line to an op that named an address
/// the producer wrote on two, which is the silent choice a patch must never make.
struct Index {
    /// Every position each key sits at.
    ///
    /// **Unordered on purpose, and said here rather than promised**: only [`Index::only`] reads a
    /// key's list and it reads the length, so keeping the positions sorted would be work nothing
    /// consumes — and a claim of order that a replacement quietly breaks is worse than no claim.
    at: std::collections::HashMap<String, Vec<usize>>,
}

impl Index {
    /// One pass over the body, which is the only pass over it a patch pays.
    fn of(lines: &[Line]) -> Self {
        let mut at: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (position, line) in lines.iter().enumerate() {
            if let Some(key) = &line.key {
                at.entry(key.clone()).or_default().push(position);
            }
        }
        Self { at }
    }

    /// The one line this key names, or the reason it names no single line.
    fn only(&self, key: &str) -> Result<usize, Problem> {
        match self.at.get(key).map(Vec::as_slice) {
            Some([only]) => Ok(*only),
            // **The count is said and not assumed to be two**, which is what the first version did:
            // a key on three lines was reported as naming two, and the published contract carries
            // this sentence word for word.
            Some(several) => Err(refused(&format!(
                "the key \"{key}\" names {} lines of this panel",
                several.len()
            ))),
            _ => Err(refused(&format!(
                "no line of this panel carries the key \"{key}\""
            ))),
        }
    }

    /// Takes a key's every position out, for a line that is being replaced.
    fn forget(&mut self, key: &str) {
        self.at.remove(key);
    }

    /// Records a key at a position, **refusing one another line already holds**.
    ///
    /// **A replacement can make a key ambiguous just as an insert can, and an earlier version of
    /// this said otherwise.** It argued that the caller had already given up the old line's key, so
    /// a collision could only be with a different line and was the insert's problem — which is the
    /// case, and is exactly why it has to be refused here too. Replacing the line keyed `mr-1` with
    /// one keyed `mr-3`, on a panel that already has an `mr-3`, left that key naming two lines and
    /// every later op on it refused for the life of the hold: the state [`Index::inserted_at`]
    /// exists to prevent, reached through the other door.
    fn remember(&mut self, key: Option<&str>, at: usize) -> Result<(), Problem> {
        if let Some(key) = key {
            if self.at.contains_key(key) {
                return Err(refused(&format!(
                    "the key \"{key}\" is already on a line of this panel, so this would give it two"
                )));
            }
            self.at.entry(key.to_owned()).or_default().push(at);
        }
        Ok(())
    }

    /// Shifts everything at or after this position up by one, then records the newcomer.
    ///
    /// **An insert whose line carries a key the document already has is refused**, and that is not
    /// tidiness. Left to succeed, it makes that key name two lines — so every later op addressing it
    /// is refused with «names two lines» for the life of the hold, and the only way out is
    /// republishing the whole document, which is the cost this verb exists to remove. A producer
    /// reaches it by an ordinary copy-paste, and by retrying a patch whose answer timed out: the
    /// channel's deadline says the window did not answer, never that it did not apply.
    fn inserted_at(&mut self, at: usize, key: Option<&str>) -> Result<(), Problem> {
        // **Asked before anything moves**, so a refusal leaves the index exactly as it found it —
        // the whole patch is dropped either way, but an index half-shifted under a `?` is the kind
        // of state a later change reaches by accident.
        if let Some(key) = key
            && self.at.contains_key(key)
        {
            return Err(refused(&format!(
                "the key \"{key}\" is already on a line of this panel, so this would give it two"
            )));
        }
        self.shift(at, 1);
        self.remember(key, at)
    }

    /// Forgets whatever sat here and shifts everything after it down by one.
    fn deleted_at(&mut self, at: usize) {
        for positions in self.at.values_mut() {
            positions.retain(|position| *position != at);
        }
        self.at.retain(|_, positions| !positions.is_empty());
        self.shift(at, -1);
    }

    /// Moves every position at or past `from` by `by`.
    ///
    /// **It walks the index and not the body**, which is what keeps a patch O(lines + ops): the
    /// index holds one entry per *keyed* line, and a document's keys are the few lines somebody
    /// meant to address.
    fn shift(&mut self, from: usize, by: isize) {
        for positions in self.at.values_mut() {
            for position in positions.iter_mut() {
                if *position >= from {
                    // **Saturating, and a `debug_assert` beside it rather than a panic.** A position
                    // walking below zero would be an off-by-one in whoever added an op, and the two
                    // ways of meeting it are not equal: in the suite it must stop the run, and in a
                    // window somebody has open it must not — a clamp paints one wrong line and a
                    // panic takes the whole terminal down. Nothing can reach it today, because a
                    // delete forgets its own position before shifting the rest.
                    debug_assert!(
                        position.checked_add_signed(by).is_some(),
                        "a key's position walked outside the body"
                    );
                    *position = position.saturating_add_signed(by);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_document;

    /// A panel with three addressable lines and one that is not addressable at all.
    fn three_keys() -> Panel {
        parse_document(
            r#"{"phase":"build","lines":[
                {"key":"title","text":"243"},
                "",
                {"key":"mr-1","text":"read it back","mark":"done"},
                {"key":"mr-3","text":"the strip","mark":"pending"}
            ]}"#,
        )
        .expect("a panel")
    }

    fn ops(text: &str) -> Result<Patch, Problem> {
        parse_ops(&serde_json::from_str(text).expect("the ops are JSON"))
    }

    fn keys(panel: &Panel) -> Vec<Option<&str>> {
        panel.lines.iter().map(|line| line.key.as_deref()).collect()
    }

    #[test]
    fn the_edits_of_one_request_see_each_others_effects() {
        let asked = ops(
            r#"[{"op":"after","key":"mr-1","line":{"key":"mr-2","text":"patch it"}},
                {"op":"replace","key":"mr-2","line":{"key":"mr-2b","text":"patch one line"}},
                {"op":"before","key":"mr-2b","line":{"key":"mr-1b","text":"between them"}},
                {"op":"delete","key":"title"}]"#,
        )
        .expect("the ops are read");

        let patched = three_keys()
            .patched_with(&asked)
            .expect("the patch applies");

        // Each op names a key an earlier one left behind: the second addresses what the first
        // inserted, the third what the second renamed. Without sequential visibility the second
        // refuses; without the index being repaired as lines move, the third lands on the wrong one.
        assert_eq!(
            keys(&patched),
            [
                None,
                Some("mr-1"),
                Some("mr-1b"),
                Some("mr-2b"),
                Some("mr-3")
            ]
        );
        assert_eq!(patched.lines[3].text, "patch one line");
    }

    /// **The address a replacement takes away is gone for the rest of the same request.**
    ///
    /// Asserted with two ops in one request, because a second call to `patched_with` rebuilds the
    /// index from the lines and would refuse whether or not the first call maintained it — a test of
    /// `Index::of` wearing this one's name. Here the deletion names a key the replacement before it
    /// took away: with the old entry left behind it still names a position, and the patch applies to
    /// a line that no longer carries that address.
    #[test]
    fn a_key_a_replacement_took_away_is_not_addressable_later_in_the_same_request() {
        let asked = ops(
            r#"[{"op":"replace","key":"mr-1","line":{"key":"mr-1b","text":"renamed"}},
                {"op":"delete","key":"mr-1"}]"#,
        )
        .expect("the ops are read");

        let refusal = three_keys()
            .patched_with(&asked)
            .expect_err("the key the replacement took away still addressed a line");

        assert!(
            refusal.to_string().contains("\"mr-1\""),
            "the refusal names the key that no longer exists: {refusal}"
        );
    }

    #[test]
    fn a_key_that_names_nothing_leaves_the_panel_exactly_as_it_was() {
        let before = three_keys();
        let asked = ops(
            r#"[{"op":"replace","key":"mr-1","line":{"key":"mr-1","text":"applied"}},
                {"op":"delete","key":"tets"}]"#,
        )
        .expect("the ops are read");

        let refusal = before
            .patched_with(&asked)
            .expect_err("a key that names nothing refuses");

        // **Only the refusal is asserted here, and that is the whole of what this level can say.**
        // `patched_with` takes `&self` and hands back a new panel, so «the original is untouched» is
        // the signature and not a property a test can fail. What atomicity means to somebody using
        // this is that the panel on screen did not move, and that is asserted where the caller
        // installs — `crates/app/src/control.rs`, through a read of the document before and after.
        assert!(
            refusal.to_string().contains("\"tets\""),
            "the refusal names the key: {refusal}"
        );
    }

    /// **An insert must not be the thing that makes a key ambiguous.**
    ///
    /// Left to succeed it bricks that address for the life of the hold: every later op naming it is
    /// refused, and the only way back is republishing the whole document — the cost this verb exists
    /// to remove. A producer reaches it by copy-paste and by retrying a patch whose answer timed out.
    #[test]
    fn an_insert_that_would_give_a_key_to_two_lines_is_refused() {
        for asked in [
            r#"[{"op":"after","key":"mr-1","line":{"key":"mr-1","text":"twice"}}]"#,
            r#"[{"op":"last","line":{"key":"title","text":"twice"}}]"#,
            r#"[{"op":"first","line":{"key":"mr-3","text":"twice"}}]"#,
        ] {
            let refusal = three_keys()
                .patched_with(&ops(asked).expect("the ops are read"))
                .expect_err("an insert made a key name two lines");
            assert!(
                refusal.to_string().contains("already on a line"),
                "the refusal says why rather than reporting an ambiguity later: {refusal}"
            );
        }

        // A key nothing carries yet goes in, which is the ordinary case this must not break.
        assert!(
            three_keys()
                .patched_with(
                    &ops(r#"[{"op":"last","line":{"key":"mr-4","text":"new"}}]"#)
                        .expect("the ops are read")
                )
                .is_ok()
        );
    }

    /// **A replacement can give a key to two lines just as an insert can**, and it reaches the same
    /// dead end: from then on every op naming that key is refused for the life of the hold, and the
    /// only way out is republishing the whole document. The insert's door was closed first and this
    /// one was argued shut in a comment — wrongly, because giving up the old address and taking a
    /// new one are two steps and the second can still collide.
    #[test]
    fn a_replacement_that_would_give_a_key_to_two_lines_is_refused() {
        let refusal = three_keys()
            .patched_with(
                &ops(r#"[{"op":"replace","key":"mr-1","line":{"key":"mr-3","text":"taken"}}]"#)
                    .expect("the ops are read"),
            )
            .expect_err("a replacement took a key another line already had");
        assert!(
            refusal.to_string().contains("already on a line"),
            "the refusal says why: {refusal}"
        );

        // **Replacing a line with one carrying its own key is the ordinary case and must not break
        // on this**: the old address is given up before the new one is taken, so it does not
        // collide with itself.
        assert!(
            three_keys()
                .patched_with(
                    &ops(r#"[{"op":"replace","key":"mr-1","line":{"key":"mr-1","text":"same"}}]"#)
                        .expect("the ops are read")
                )
                .is_ok()
        );
    }

    /// The expiry a patch sets saturates exactly as the document's own reader does — **which is
    /// what the two of them sharing one function is for**, and the only value tested until now was
    /// five minutes.
    #[test]
    fn an_absurd_expiry_set_by_a_patch_saturates_where_the_documents_own_reader_does() {
        let asked = ops(&format!(
            r#"[{{"op":"set","field":"stale_after_minutes","value":{}}}]"#,
            u64::MAX
        ))
        .expect("the ops are read");

        assert_eq!(
            three_keys()
                .patched_with(&asked)
                .expect("the patch applies")
                .stale_after,
            Duration::from_secs(u64::MAX)
        );
    }

    /// The count in the ambiguity's sentence **is the count**, and the published contract carries
    /// this wording word for word.
    #[test]
    fn a_key_on_three_lines_is_not_reported_as_naming_two() {
        let thrice = parse_document(
            r#"{"lines":[{"key":"m","text":"a"},{"key":"m","text":"b"},{"key":"m","text":"c"}]}"#,
        )
        .expect("a panel");

        let refusal = thrice
            .patched_with(&ops(r#"[{"op":"delete","key":"m"}]"#).expect("the ops are read"))
            .expect_err("an ambiguous key refuses");

        assert!(
            refusal.to_string().contains("names 3 lines"),
            "the refusal counts the lines rather than assuming two: {refusal}"
        );
    }

    #[test]
    fn a_key_that_names_two_lines_refuses_the_op_that_names_it() {
        let twice = parse_document(
            r#"{"lines":[{"key":"mr-2","text":"one"},{"key":"mr-2","text":"other"}]}"#,
        )
        .expect("a panel");
        let asked = ops(r#"[{"op":"delete","key":"mr-2"}]"#).expect("the ops are read");

        let refusal = twice
            .patched_with(&asked)
            .expect_err("an ambiguous key refuses");

        assert!(
            refusal.to_string().contains("names 2 lines"),
            "the refusal says the key is ambiguous: {refusal}"
        );
    }

    #[test]
    fn the_ends_of_the_body_are_reachable_without_naming_anything() {
        let asked = ops(
            r#"[{"op":"first","line":"over the top"},{"op":"last","line":{"text":"under it"}}]"#,
        )
        .expect("the ops are read");

        let patched = three_keys()
            .patched_with(&asked)
            .expect("the patch applies");

        assert_eq!(patched.lines[0].text, "over the top");
        assert_eq!(patched.lines[5].text, "under it");
        // The keys that were there still address what they addressed: an insertion at either end
        // shifts every position after it, and the index is what has to survive that.
        let after = ops(r#"[{"op":"delete","key":"mr-1"}]"#).expect("the ops are read");
        let again = patched.patched_with(&after).expect("the patch applies");
        assert_eq!(
            keys(&again),
            [None, Some("title"), None, Some("mr-3"), None]
        );
    }

    /// **A delete moves every address after it down, inside the same request.**
    ///
    /// Nothing else reaches that repair. Every other test here either deletes last, or deletes in a
    /// request of its own — where the next `patched_with` rebuilds the index from the lines and would
    /// pass whether or not the deletion had maintained anything. Left out, the op after a delete
    /// resolves its key against a position one too far along and edits the line below the one it
    /// named, with the producer told `{"ok":true}`.
    #[test]
    fn a_delete_moves_every_address_after_it_down_in_the_same_request() {
        let four = parse_document(
            r#"{"lines":[{"key":"a","text":"first"},{"key":"b","text":"second"},
                {"key":"c","text":"third"},{"key":"d","text":"fourth"}]}"#,
        )
        .expect("a panel");
        let asked = ops(r#"[{"op":"delete","key":"a"},
                {"op":"replace","key":"c","line":{"key":"c","text":"landed"}}]"#)
        .expect("the ops are read");

        let patched = four.patched_with(&asked).expect("the patch applies");

        assert_eq!(keys(&patched), [Some("b"), Some("c"), Some("d")]);
        assert_eq!(
            patched
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["second", "landed", "fourth"],
            "the replacement landed where the body was before the delete, not where it is now"
        );
    }

    /// **A key a delete freed is free again for the rest of the same request.**
    ///
    /// An insert refuses a key the document already carries, and the index is what it asks. A
    /// deletion that leaves that key's entry behind — emptied but still there — has the insert told
    /// the address is taken on a panel that no longer carries it, and the only way to reuse an
    /// address would be republishing the whole document: the cost this verb exists to remove. Moving
    /// a line by deleting it and putting it back is the ordinary shape of that mistake.
    #[test]
    fn a_key_a_delete_freed_can_be_given_to_a_new_line_in_the_same_request() {
        let asked = ops(r#"[{"op":"delete","key":"mr-1"},
                {"op":"last","line":{"key":"mr-1","text":"moved to the end"}}]"#)
        .expect("the ops are read");

        let patched = three_keys()
            .patched_with(&asked)
            .expect("the address the delete gave back was still held against the insert");

        assert_eq!(
            keys(&patched),
            [Some("title"), None, Some("mr-3"), Some("mr-1")]
        );
        assert_eq!(patched.lines[3].text, "moved to the end");
    }

    /// **Two inserts at one position: the first one is still where the index says it is.**
    ///
    /// The second insert has to move the first along with everything else sitting **at** that
    /// position, and that boundary is the whole of what the shift decides. Excluded, the two inserted
    /// lines share a position and the op addressing the earlier one edits its neighbour instead —
    /// silently, and answered `{"ok":true}`.
    #[test]
    fn a_second_insert_at_the_same_position_moves_the_first_one_along() {
        let asked = ops(
            r#"[{"op":"first","line":{"key":"under","text":"second in"}},
                {"op":"first","line":{"key":"over","text":"third in"}},
                {"op":"replace","key":"under","line":{"key":"under","text":"landed"}}]"#,
        )
        .expect("the ops are read");

        let patched = three_keys()
            .patched_with(&asked)
            .expect("the patch applies");

        assert_eq!(
            keys(&patched),
            [
                Some("over"),
                Some("under"),
                Some("title"),
                None,
                Some("mr-1"),
                Some("mr-3")
            ]
        );
        assert_eq!(patched.lines[1].text, "landed");
        assert_eq!(
            patched.lines[0].text, "third in",
            "the replacement edited the line inserted over the one it named"
        );
    }

    #[test]
    fn a_scalar_is_set_and_null_puts_back_what_absence_produces() {
        let asked = ops(r#"[{"op":"set","field":"attention","value":"done"},
                {"op":"set","field":"header","value":false},
                {"op":"set","field":"stale_after_minutes","value":5},
                {"op":"set","field":"phase","value":null}]"#)
        .expect("the ops are read");

        let patched = three_keys()
            .patched_with(&asked)
            .expect("the patch applies");

        assert_eq!(patched.attention, Some(Attention::Done));
        assert!(!patched.header);
        assert_eq!(patched.stale_after, Duration::from_secs(5 * 60));
        assert_eq!(
            patched.phase, None,
            "a null is «as if it had not been said»"
        );

        // **And a phase given a real word lands as that word.** Without this, an arm that cleared
        // the field whatever it was handed would satisfy every assertion above — the null cases are
        // all that the rest of this test looks at.
        let named =
            ops(r#"[{"op":"set","field":"phase","value":"review"}]"#).expect("the ops are read");
        assert_eq!(
            three_keys()
                .patched_with(&named)
                .expect("the patch applies")
                .phase
                .as_deref(),
            Some("review")
        );
    }

    #[test]
    fn null_on_the_two_fields_that_have_a_default_puts_the_default_back() {
        let changed = ops(r#"[{"op":"set","field":"header","value":false},
                {"op":"set","field":"stale_after_minutes","value":5}]"#)
        .expect("the ops are read");
        let cleared = ops(r#"[{"op":"set","field":"header","value":null},
                {"op":"set","field":"stale_after_minutes","value":null}]"#)
        .expect("the ops are read");

        let patched = three_keys()
            .patched_with(&changed)
            .expect("the patch applies")
            .patched_with(&cleared)
            .expect("the patch applies");

        // Absence is what produces the default, so a null has to land on the default and not on
        // `false` or on zero minutes — which is where a bare `unwrap_or_default` would put it.
        assert!(patched.header);
        assert_eq!(patched.stale_after, STALE_AFTER);
    }

    #[test]
    fn a_line_an_op_carries_is_read_with_the_documents_own_tolerance() {
        let asked = ops(
            r#"[{"op":"replace","key":"mr-1","line":{"key":"mr-1","text":"still here","mark":"finito","indent":9000}}]"#,
        )
        .expect("a bad mark inside a line does not refuse the patch");

        let patched = three_keys()
            .patched_with(&asked)
            .expect("the patch applies");

        assert_eq!(patched.lines[2].text, "still here");
        assert_eq!(
            patched.lines[2].mark, None,
            "the word nobody knows is dropped"
        );
        assert_eq!(
            patched.lines[2].indent as u64,
            crate::MAX_INDENT,
            "and the indent is clipped exactly as the document's reader clips it"
        );
    }

    #[test]
    fn a_field_a_set_names_is_not_tolerated_the_way_a_line_is() {
        let refusal = ops(r#"[{"op":"set","field":"attention","value":"waiting"}]"#)
            .expect_err("an attention nobody knows refuses the patch");

        // The same word inside a published document is dropped and the panel still paints. A request
        // is an instruction, and this is the divergence the contract document states.
        assert!(
            refusal.to_string().contains("\"waiting\""),
            "the refusal names the value: {refusal}"
        );
        let dropped = parse_document(r#"{"attention":"waiting","lines":["x"]}"#)
            .expect("the same word in a document parses");
        assert_eq!(dropped.attention, None);
    }

    #[test]
    fn buttons_are_refused_by_their_own_name() {
        let refusal = ops(r#"[{"op":"set","field":"actions","value":null}]"#)
            .expect_err("actions is not patchable");

        assert!(
            refusal
                .to_string()
                .contains("only ever set with the whole document"),
            "the refusal says why rather than calling it unknown: {refusal}"
        );
    }

    #[test]
    fn each_thing_decidable_without_the_document_is_decided_without_it() {
        for (asked, what) in [
            (
                r#"[{"op":"rewrite","key":"a","line":"x"}]"#,
                "an op nobody serves",
            ),
            (
                r#"[{"op":"set","field":"colour","value":null}]"#,
                "a field nobody has",
            ),
            (
                r#"[{"op":"replace","line":"x"}]"#,
                "an addressing op with no key",
            ),
            (r#"[{"op":"first"}]"#, "a carrying op with no line"),
            (r#"[{"op":"first","line":7}]"#, "a line that is not a line"),
            (
                r#"[{"op":"set","field":"header","value":"yes"}]"#,
                "a value of the wrong kind",
            ),
            (r#"[{"op":"set","field":"phase"}]"#, "a set with no value"),
            (
                r#"[{"op":"set","field":"stale_after_minutes","value":1.5}]"#,
                "a fractional number of minutes",
            ),
            (
                r#"[{"op":"set","field":"stale_after_minutes","value":-5}]"#,
                "a negative number of minutes",
            ),
            (r#"[]"#, "a patch carrying no ops at all"),
            (r#"[{"key":"a"}]"#, "an element that names no op"),
            (r#"{"op":"first","line":"x"}"#, "ops that are not a list"),
        ] {
            assert!(
                ops(asked).is_err(),
                "{what} is refused before any document is reached"
            );
        }
    }

    #[test]
    fn more_ops_than_the_ceiling_are_refused_and_never_truncated() {
        let one = r#"{"op":"last","line":"x"}"#;
        let at_the_ceiling = format!("[{}]", [one; MAX_OPS].join(","));
        let one_over = format!("[{}]", [one; MAX_OPS + 1].join(","));

        assert_eq!(
            ops(&at_the_ceiling)
                .expect("the ceiling itself is served")
                .len(),
            MAX_OPS
        );
        let refusal = ops(&one_over).expect_err("one more is refused");
        // A truncated instruction is an edit that silently did not happen with the producer told
        // `{"ok":true}`, so the refusal has to name the count rather than quietly dropping the tail.
        assert!(
            refusal.to_string().contains(&(MAX_OPS + 1).to_string()),
            "the refusal says how many arrived: {refusal}"
        );
    }

    #[test]
    fn a_patch_over_a_keyless_document_reaches_only_the_ends() {
        let keyless = parse_document(r#"{"lines":["one","two"]}"#).expect("a panel");

        let ends = ops(r#"[{"op":"first","line":"nought"}]"#).expect("the ops are read");
        assert_eq!(
            keyless
                .patched_with(&ends)
                .expect("the ends are reachable")
                .lines
                .len(),
            3
        );

        let addressed = ops(r#"[{"op":"delete","key":"one"}]"#).expect("the ops are read");
        assert!(
            keyless.patched_with(&addressed).is_err(),
            "a line's text is not an address: nothing carries that key"
        );
    }
}
