//! A project's roadmap: the plan document an agent publishes for the repository it works in,
//! written to `<state>/projects/<key>/plan.json`, which is what the board reads.
//!
//! **Not held like a panel.** A panel belongs to one console and one session; a plan belongs to the
//! repository, and whichever console plans the work publishes it — the newest publication wins.
//! The repository is the one this process runs in, or the one the call names.

use std::path::{Path, PathBuf};
use std::process::Command;

use panel_contract::{PLAN_FILE, PROJECTS_DIR, parse_plan, project_key, serialize_plan};

use crate::pane::{Answered, state_base, write_atomically};

const NO_PLAN: &str = "this project has no plan published yet";

#[derive(Debug)]
pub struct Project {
    dir: PathBuf,
}

impl Project {
    /// The project of the repository at `repo`, or of this process's working directory.
    pub fn of(repo: Option<&str>) -> Result<Self, String> {
        let at = match repo {
            Some(path) if Path::new(path).is_absolute() => PathBuf::from(path),
            Some(path) => return Err(format!("\"repo\" must be an absolute path, not {path:?}")),
            None => std::env::current_dir()
                .map_err(|failure| format!("this process has no working directory ({failure})"))?,
        };
        let root = repository_root(&at)?;
        Ok(Self::at(state_base()?.join(PROJECTS_DIR).join(project_key(&root))))
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file(&self) -> PathBuf {
        self.dir.join(PLAN_FILE)
    }

    pub fn publish(&self, document: &serde_json::Value) -> Answered {
        let plan = match parse_plan(&document.to_string()) {
            Ok(plan) => plan,
            Err(problem) => return Answered::Failed(problem.to_string()),
        };
        let text = match serialize_plan(&plan) {
            Ok(text) => text,
            Err(problem) => return Answered::Failed(problem.to_string()),
        };
        match write_atomically(&self.dir, &self.file(), &text) {
            Ok(()) => Answered::Ok,
            Err(failure) => Answered::Failed(format!("could not write the plan ({failure})")),
        }
    }

    pub fn read(&self) -> Answered {
        match std::fs::read_to_string(self.file()) {
            Ok(text) => match parse_plan(&text).and_then(|plan| serialize_plan(&plan)) {
                Ok(text) => Answered::Document(text),
                Err(problem) => Answered::Failed(problem.to_string()),
            },
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
                Answered::Failed(NO_PLAN.to_owned())
            }
            Err(failure) => Answered::Failed(format!("could not read the plan ({failure})")),
        }
    }

    pub fn retire(&self) -> Answered {
        match std::fs::remove_file(self.file()) {
            Ok(()) => Answered::Retired,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
                Answered::Failed(NO_PLAN.to_owned())
            }
            Err(failure) => Answered::Failed(format!("could not remove the plan ({failure})")),
        }
    }
}

/// The main checkout's folder of the repository `at` belongs to — the same for all its worktrees.
///
/// **Asked of git, not worked out from the path**: git knows where a worktree's shared directory
/// is, and that is a fact on disk rather than a guess about anybody's work.
fn repository_root(at: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(at)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .map_err(|failure| format!("could not run git ({failure})"))?;
    if !output.status.success() {
        return Err(format!("{} is not inside a git repository", at.display()));
    }
    let common = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    Ok(root_of(&common))
}

/// `/repo/.git` → `/repo`; a bare repository is its own root.
fn root_of(common: &Path) -> PathBuf {
    match (common.file_name(), common.parent()) {
        (Some(name), Some(parent)) if name == ".git" => parent.to_path_buf(),
        _ => common.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bellwether-plan-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_plan_reaches_the_file_the_board_reads_and_reads_back() {
        let dir = scratch("publish");
        let project = Project::at(dir.clone());

        let answered = project.publish(&json!({"title": "Leave Quaderno", "milestones": [{"id": "H3", "items": ["a"]}]}));

        assert_eq!(answered, Answered::Ok);
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("plan.json")).expect("written")).expect("JSON");
        assert_eq!(written["milestones"][0]["id"], "H3");
        let Answered::Document(text) = project.read() else { panic!("not read back") };
        assert!(text.contains("Leave Quaderno"), "{text}");
    }

    #[test]
    fn a_document_that_is_not_a_plan_writes_nothing() {
        let dir = scratch("refused");
        assert!(matches!(Project::at(dir.clone()).publish(&json!({"title": "x"})), Answered::Failed(_)));
        assert!(!dir.join("plan.json").exists());
    }

    #[test]
    fn retiring_removes_the_plan_and_says_when_there_was_none() {
        let dir = scratch("retire");
        let project = Project::at(dir.clone());
        project.publish(&json!({"milestones": []}));

        assert_eq!(project.retire(), Answered::Retired);
        assert!(!dir.join("plan.json").exists());
        assert_eq!(project.retire(), Answered::Failed(NO_PLAN.to_owned()));
    }

    #[test]
    fn worktrees_and_the_main_checkout_share_one_root() {
        assert_eq!(root_of(Path::new("/p/tax-platform/.git")), PathBuf::from("/p/tax-platform"));
        assert_eq!(root_of(Path::new("/p/bare.git")), PathBuf::from("/p/bare.git"));
    }

    /// The real git, on this very repository: the root it answers from a subfolder is the folder
    /// holding `.git`, which is what makes a console in `crates/` land in the same project.
    #[test]
    fn git_names_the_root_from_anywhere_inside() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = repository_root(here).expect("this crate lives in a repository");
        assert!(root.join(".git").exists(), "{}", root.display());
        assert!(here.starts_with(&root));
    }

    #[test]
    fn a_relative_repo_is_refused() {
        assert!(Project::of(Some("tax-platform")).is_err());
    }
}
