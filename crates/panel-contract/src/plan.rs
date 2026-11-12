//! What `plan.json` says: a project's roadmap, published by whoever plans the work.
//!
//! **One per repository, not one per console.** A panel tells what one console is doing; a plan
//! tells where the whole project is going, so every console working in the repository — its
//! worktrees included — hangs from the same one. A console says which milestone it works on with
//! its panel's `milestone` and `deliverable`; nothing here guesses it.
//!
//! **Read the way a panel is read**: tolerant field by field, refused only when it is not JSON or
//! carries no `milestones`. A milestone without an `id` cannot be addressed by any console, so it
//! is dropped rather than painted under an invented name.
//!
//! **The plan says each deliverable's state; the reader only counts.** How far a milestone is
//! along is the number of its deliverables marked `done` over all of them — a sum of what the
//! producer declared, never a judgement of the reader's own.

use std::path::Path;

use crate::panel::{MAX_BYTES, Mark, Problem};

/// The file a plan lives in, inside its project's directory.
pub const PLAN_FILE: &str = "plan.json";

/// The directory under the state base that holds one directory per project.
pub const PROJECTS_DIR: &str = "projects";

/// The most milestones a plan keeps. A real roadmap has fifteen; the rest are cut, not refused.
pub const MAX_MILESTONES: usize = 64;

/// The most deliverables one milestone keeps, and the most entries `outside` keeps.
pub const MAX_ITEMS: usize = 128;

/// A project's roadmap.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    pub version: Option<u64>,
    /// What the whole plan delivers — `Leave Quaderno`. Painted beside the project's name.
    pub title: Option<String>,
    /// Where the plan was read from, as the producer names it — `docs/roadmap.md`.
    pub source: Option<String>,
    /// In the order the producer gives them, which is the order they are painted.
    pub milestones: Vec<Milestone>,
    /// Work the plan depends on that is not code — a certificate, a contract, a decision.
    pub outside: Vec<Item>,
}

/// One milestone of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Milestone {
    /// What a console's `milestone` names it by. Never empty.
    pub id: String,
    pub title: Option<String>,
    /// The condition that closes it, as the plan writes it.
    pub done_when: Option<String>,
    /// The ids of the milestones it waits for, as the plan writes them. Painted, never enforced.
    pub depends_on: Vec<String>,
    pub items: Vec<Item>,
}

/// A deliverable of a milestone, or an entry of the plan's `outside`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Item {
    /// What a console's `deliverable` names it by.
    pub id: Option<String>,
    pub text: String,
    /// Its state, in the panel's own vocabulary: `done`, `current`, `pending`, `wait`, `block`.
    pub mark: Option<Mark>,
    /// Short label painted beside it — its ticket, `MT-16461`.
    pub reference: Option<String>,
    pub link: Option<String>,
}

impl Milestone {
    /// How many of its deliverables the plan marks done, and how many it has.
    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
        let done = self
            .items
            .iter()
            .filter(|item| item.mark == Some(Mark::Done))
            .count();
        (done, self.items.len())
    }
}

/// Reads a plan, tolerating per field.
pub fn parse_plan(text: &str) -> Result<Plan, Problem> {
    if text.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    let raw: serde_json::Value =
        serde_json::from_str(text).map_err(|failure| Problem::Invalid(failure.to_string()))?;
    let milestones = raw
        .get("milestones")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Problem::Invalid("carries no \"milestones\"".to_owned()))?;

    Ok(Plan {
        version: raw.get("v").and_then(serde_json::Value::as_u64),
        title: string(&raw, "title"),
        source: string(&raw, "source"),
        milestones: milestones
            .iter()
            .filter_map(milestone)
            .take(MAX_MILESTONES)
            .collect(),
        outside: items(&raw, "outside"),
    })
}

/// Writes a plan back out as one line of JSON, **emitting no field the plan did not carry**, or
/// refuses it for being over [`MAX_BYTES`].
pub fn serialize_plan(plan: &Plan) -> Result<String, Problem> {
    let mut out = serde_json::Map::new();
    if let Some(version) = plan.version {
        out.insert("v".to_owned(), version.into());
    }
    put(&mut out, "title", plan.title.as_deref());
    put(&mut out, "source", plan.source.as_deref());
    out.insert(
        "milestones".to_owned(),
        plan.milestones.iter().map(milestone_value).collect(),
    );
    if !plan.outside.is_empty() {
        out.insert(
            "outside".to_owned(),
            plan.outside.iter().map(item_value).collect(),
        );
    }
    let text = serde_json::Value::Object(out).to_string();
    if text.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    Ok(text)
}

/// The name of a project's directory, **from the root of its repository**.
///
/// The root is the main checkout's folder — the one whose `.git` every worktree shares — so all
/// the worktrees of one repository land in one directory. The whole absolute path goes into the
/// name, with `%` and `/` escaped, so two repositories called the same in different folders do not
/// share a plan. The board builds the same name in JavaScript; the two must not drift.
#[must_use]
pub fn project_key(root: &Path) -> String {
    root.to_string_lossy()
        .replace('%', "%25")
        .replace('/', "%2F")
}

fn milestone(raw: &serde_json::Value) -> Option<Milestone> {
    let id = string(raw, "id").filter(|id| !id.is_empty())?;
    Some(Milestone {
        id,
        title: string(raw, "title"),
        done_when: string(raw, "done_when"),
        depends_on: raw
            .get("depends_on")
            .and_then(serde_json::Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .take(MAX_MILESTONES)
                    .collect()
            })
            .unwrap_or_default(),
        items: items(raw, "items"),
    })
}

fn items(raw: &serde_json::Value, field: &str) -> Vec<Item> {
    raw.get(field)
        .and_then(serde_json::Value::as_array)
        .map(|declared| declared.iter().filter_map(item).take(MAX_ITEMS).collect())
        .unwrap_or_default()
}

/// A deliverable is a string or an object with `text`; anything else is not one.
fn item(raw: &serde_json::Value) -> Option<Item> {
    if let Some(text) = raw.as_str() {
        return Some(Item {
            text: text.to_owned(),
            ..Item::default()
        });
    }
    if !raw.is_object() {
        return None;
    }
    Some(Item {
        id: string(raw, "id"),
        text: string(raw, "text").unwrap_or_default(),
        mark: raw
            .get("mark")
            .and_then(serde_json::Value::as_str)
            .and_then(|name| Mark::ALL.into_iter().find(|known| known.name() == name)),
        reference: string(raw, "ref"),
        link: string(raw, "link"),
    })
}

fn milestone_value(milestone: &Milestone) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    out.insert("id".to_owned(), milestone.id.clone().into());
    put(&mut out, "title", milestone.title.as_deref());
    put(&mut out, "done_when", milestone.done_when.as_deref());
    if !milestone.depends_on.is_empty() {
        out.insert("depends_on".to_owned(), milestone.depends_on.clone().into());
    }
    if !milestone.items.is_empty() {
        out.insert(
            "items".to_owned(),
            milestone.items.iter().map(item_value).collect(),
        );
    }
    serde_json::Value::Object(out)
}

fn item_value(item: &Item) -> serde_json::Value {
    let bare = item.id.is_none() && item.mark.is_none() && item.reference.is_none() && item.link.is_none();
    if bare {
        return item.text.clone().into();
    }
    let mut out = serde_json::Map::new();
    put(&mut out, "id", item.id.as_deref());
    if !item.text.is_empty() {
        out.insert("text".to_owned(), item.text.clone().into());
    }
    if let Some(mark) = item.mark {
        out.insert("mark".to_owned(), mark.name().into());
    }
    put(&mut out, "ref", item.reference.as_deref());
    put(&mut out, "link", item.link.as_deref());
    serde_json::Value::Object(out)
}

fn put(out: &mut serde_json::Map<String, serde_json::Value>, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        out.insert(name.to_owned(), value.into());
    }
}

fn string(raw: &serde_json::Value, field: &str) -> Option<String> {
    raw.get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROADMAP: &str = r#"{
        "v": 1, "title": "Leave Quaderno", "source": "docs/roadmap.md",
        "milestones": [
            {"id": "H2", "title": "TaxIds", "items": [{"text": "a", "mark": "done"}]},
            {"id": "H3", "title": "TaxRates", "done_when": "France on 1 Jan 2025 is right",
             "depends_on": ["H0"],
             "items": [
                {"id": "catalog", "text": "Jurisdiction catalog", "mark": "done", "ref": "MT-16456"},
                {"id": "TAX-06", "text": "Manual load with approval", "mark": "pending"},
                "Backup sources"
             ]}
        ],
        "outside": ["Company certificate", {"text": "RSM", "mark": "pending"}]
    }"#;

    #[test]
    fn a_roadmap_is_read_in_the_order_the_producer_gives() {
        let plan = parse_plan(ROADMAP).expect("a plan");

        assert_eq!(plan.title.as_deref(), Some("Leave Quaderno"));
        let ids: Vec<&str> = plan.milestones.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["H2", "H3"]);
        let h3 = &plan.milestones[1];
        assert_eq!(h3.depends_on, ["H0"]);
        assert_eq!(h3.items[1].id.as_deref(), Some("TAX-06"));
        assert_eq!(h3.items[2].text, "Backup sources");
        assert_eq!(plan.outside.len(), 2);
    }

    /// Progress is what the plan marks `done`, so a `current` deliverable — being worked on, not
    /// finished — does not count, and an unmarked one counts as not done.
    #[test]
    fn progress_counts_only_what_the_plan_marks_done() {
        let plan = parse_plan(
            r#"{"milestones":[{"id":"H3","items":[
                {"text":"a","mark":"done"},{"text":"b","mark":"current"},
                {"text":"c","mark":"done"},"d"]}]}"#,
        )
        .expect("a plan");
        assert_eq!(plan.milestones[0].progress(), (2, 4));
    }

    #[test]
    fn a_milestone_no_console_can_name_is_dropped() {
        let plan = parse_plan(
            r#"{"milestones":[{"title":"no id"},{"id":"","title":"empty id"},{"id":"H1"},7]}"#,
        )
        .expect("a plan");
        let ids: Vec<&str> = plan.milestones.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["H1"]);
    }

    #[test]
    fn without_milestones_it_is_not_a_plan() {
        assert!(matches!(parse_plan(r#"{"title":"x"}"#), Err(Problem::Invalid(_))));
        assert!(matches!(parse_plan("not json"), Err(Problem::Invalid(_))));
    }

    #[test]
    fn an_unknown_state_falls_to_nothing_and_the_deliverable_stands() {
        let plan = parse_plan(r#"{"milestones":[{"id":"H1","items":[{"text":"a","mark":"doing"}]}]}"#)
            .expect("a plan");
        assert_eq!(plan.milestones[0].items[0].mark, None);
        assert_eq!(plan.milestones[0].items[0].text, "a");
    }

    #[test]
    fn surplus_milestones_and_deliverables_are_cut_not_refused() {
        let many_items: Vec<String> = (0..=MAX_ITEMS).map(|at| format!("\"d{at}\"")).collect();
        let many: Vec<String> = (0..=MAX_MILESTONES)
            .map(|at| format!(r#"{{"id":"M{at}","items":[{}]}}"#, many_items.join(",")))
            .collect();
        let plan = parse_plan(&format!(r#"{{"milestones":[{}]}}"#, many.join(",")))
            .expect("a plan");
        assert_eq!(plan.milestones.len(), MAX_MILESTONES);
        assert_eq!(plan.milestones[0].items.len(), MAX_ITEMS);
    }

    #[test]
    fn a_plan_over_the_ceiling_is_refused() {
        let text = format!(r#"{{"title":"{}","milestones":[]}}"#, "x".repeat(MAX_BYTES as usize));
        assert!(matches!(parse_plan(&text), Err(Problem::TooBig)));
    }

    #[test]
    fn a_plan_survives_the_round_trip_and_never_grows() {
        let plan = parse_plan(ROADMAP).expect("a plan");
        let written = serialize_plan(&plan).expect("it fits");

        assert_eq!(parse_plan(&written).expect("read back"), plan);
        assert!(written.len() < ROADMAP.len(), "{written}");
        assert!(!written.contains("null"), "a field the plan did not carry was written: {written}");
    }

    /// Worktrees share the main checkout's root, so this is what puts them in one project; and the
    /// whole path is the name, so two repositories with the same folder name do not share a plan.
    #[test]
    fn the_project_key_is_the_whole_root_with_its_separators_escaped() {
        assert_eq!(
            project_key(Path::new("/home/u/Proyectos/tax-platform")),
            "%2Fhome%2Fu%2FProyectos%2Ftax-platform"
        );
        assert_ne!(
            project_key(Path::new("/a/app")),
            project_key(Path::new("/b/app"))
        );
        assert_eq!(project_key(Path::new("/a/100%")), "%2Fa%2F100%25");
    }
}
