#![deny(rustdoc::broken_intra_doc_links)]
// **A broken rustdoc link is invisible to the compiler, and half the repository is rustdoc.** When
// this was put in, the tree already had one pointing at a constant that an earlier rename had left
// behind: nobody noticed because `cargo doc` warns and does not fail. With the `deny` it fails, and
// the milestone that renames 46,458 lines is going to be grateful for it.
//
// It fires on `cargo doc`, **not** on `cargo test` — this repository has no continuous integration,
// so it still has to be invoked. What changes is that now it breaks instead of warning.
//! Types for `panel.json`, reading and validation, and **the directory where that file lives**.
//!
//! It depends on nothing visual: if the painting layer is rewritten, this crate keeps serving.
//!
//! The directory is here and not in whoever launches the consoles for the reason the line above
//! gives: **nothing about generating a name, creating a directory and deleting it has anything to do
//! with painting**, so putting it in the crate that is going to be rewritten the day the interface
//! framework changes would force it to be rewritten along with it for no reason at all. And *where*
//! the state is published is part of the contract, just as much as *what* is published.

use std::ffi::OsStr;
use std::fs::DirBuilder;
use std::io;
use std::path::{Path, PathBuf};

mod dropped;
mod panel;
mod patch;
mod plan;

pub use dropped::Dropped;
pub use panel::{
    Action, Attention, Line, MAX_ACTIONS, MAX_BYTES, MAX_INDENT, MAX_SEND, MAX_SUMMARY, Mark, Panel, Problem,
    STALE_AFTER, Style, is_shown_as_itself, parse_document, parse_document_noting, read,
    read_if_newer, serialize,
};
pub use patch::{MAX_OPS, Op, Patch, Scalar, parse_ops, parse_ops_noting};
pub use plan::{
    Item, MAX_ITEMS, MAX_MILESTONES, Milestone, PLAN_FILE, PROJECTS_DIR, Plan, parse_plan, project_key,
    serialize_plan,
};

/// The only file the contract puts inside the directory. It is written by whoever does the work,
/// not by this crate.
///
/// **Public because it is needed outside.** The watcher that arrives with F1.6 runs in the
/// application and has to discard the notifications from any other path, so it needs to name this
/// file; without exporting it, the string would be written out by hand in every place and the
/// contract would stop living in a single one.
pub const PANEL_FILE: &str = "panel.json";

/// The other thing the contract puts inside the directory: the endpoint a producer can speak to
/// instead of writing the file.
///
/// **It is here, beside [`PANEL_FILE`], for the reason the crate's header gives**: *where* the state
/// is published is part of the contract, so the two transports' names live in the same place. It is
/// also what [`remove`] has to know about — see that function for what a second name in this
/// directory costs if it is forgotten.
///
/// **This is the POSIX half of the endpoint's name, and only that.** A pipe lives in the machine's
/// namespace rather than in a directory, so on Windows there is nothing here to join a path to and
/// whoever binds builds the name from the pane's identifier instead; nothing on that platform reads
/// this constant. Which of the two does the separating is the transport's business, and the
/// `panel-control` crate is where it is decided.
pub const CONTROL_SOCKET: &str = "control.sock";

/// The names of the contract's two variables, in a single place.
///
/// They are here, and not in every place that writes them, because they are needed **also when there
/// is no directory**: then they have to be removed from the program's environment, and to remove
/// them you have to know what they are called.
pub const ENV_NAMES: [&str; 2] = ["PANEL_ID", "PANEL_DIR"];

/// The directory of a split: its identifier and the path where its producer publishes the state.
///
/// It is born with the directory already created and **deletes it when it dies**. That is the reason
/// it is a type and not two loose functions: the `Drop` covers paths that an explicit cleanup would
/// miss —the half-built error of whoever uses it, and the panic, where the interface framework does
/// not run its shutdown hook but destructors do run.
#[derive(Debug)]
pub struct PanelDir {
    id: String,
    path: PathBuf,
}

impl PanelDir {
    /// Generates the identifier, creates its directory and leaves it ready for the producer to write.
    ///
    /// **Creation is the uniqueness check**: the directory is created without recursion, so if the
    /// name already existed this fails instead of sharing the directory with another split —and
    /// sharing it would silently break the per-identifier scoping the rest depends on. That is also
    /// why the path adds **a single component**: there is no intermediate segment that would have to
    /// be created first, nor that a third party could leave prepared in a shared temporary.
    pub fn create() -> io::Result<Self> {
        let id = generate_id()?;
        let path = base_dir().join(format!("ivy-{id}"));
        create_dir(&path)?;

        Ok(Self { id, path })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The two variables the split's process inherits.
    ///
    /// **The array is of two and not a list, because that number is what gets reviewed.** Whoever
    /// launches the program accepts an open list of pairs —it has to accept it, because it does not
    /// know what a panel is—, so the ceiling is not there: it is in that the only thing passed to it
    /// comes from here. Adding a third variable forces a change to this type, in the contract crate;
    /// with a list built at the call site, injecting one more —a credential, for example— would be
    /// data and not a change that someone would have to approve.
    pub fn env(&self) -> [(&'static str, &OsStr); 2] {
        [
            (ENV_NAMES[0], OsStr::new(&self.id)),
            (ENV_NAMES[1], self.path.as_os_str()),
        ]
    }
}

impl Drop for PanelDir {
    /// Deletes what the contract puts in the directory, and then the directory.
    ///
    /// **Without recursion, on purpose.** The directory belongs to this crate, but its content
    /// belongs to the producer: a recursive delete would wipe out whatever it had left there, and
    /// would turn any badly built path into data loss instead of an error. If something that is not
    /// the contract's remains, the directory survives and it is said where.
    fn drop(&mut self) {
        // A notice is emitted because the fallback to a system temporary is **not** emptied when the
        // session closes, unlike the user's runtime directory: without this line, a leftover there
        // would be invisible and would pile up as soon as there are producers writing for real.
        if let Some(failure) = remove(&self.path) {
            eprintln!(
                "the panel directory {} remains ({failure})",
                self.path.display()
            );
        }
    }
}

/// Deletes the contract file and the directory, and returns **what deserves a notice**.
///
/// That the directory is no longer there does not deserve one: the user has a shell inside their
/// console and may have deleted it themselves. That something not belonging to the contract remains,
/// yes.
///
/// It is separate from the destructor so it can be tested: a destructor returns nothing, and the
/// decision of what is kept quiet and what is said is exactly what has to be assertable.
///
/// **Everything the contract puts here is deleted here, and that is why the list is two names
/// rather than one.** The deletion stays non-recursive for the reason above, so anything of this
/// crate's own left behind makes `remove_dir` fail and prints the notice on **every** console close.
fn remove(path: &Path) -> Option<io::Error> {
    // That it is not there is the normal case: until someone publishes, the directory is empty.
    let _ = std::fs::remove_file(path.join(PANEL_FILE));
    // The endpoint is only there on the platforms where it is a file at all, and only once
    // something bound it: the same «normal case» as the line above, and for the same reason its
    // failure is dropped.
    let _ = std::fs::remove_file(path.join(CONTROL_SOCKET));

    match std::fs::remove_dir(path) {
        Ok(()) => None,
        Err(failure) if failure.kind() == io::ErrorKind::NotFound => None,
        Err(failure) => Some(failure),
    }
}

/// Creates the directory, and **fails if it already exists**.
///
/// Without recursion for the two reasons: there is no intermediate segment to create, and the
/// creation mode that does accept intermediate segments also does not complain when the path is
/// already there — it would adopt a directory this crate did not make.
fn create_dir(path: &Path) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    // Without this the mode comes from the session's umask —measured: `0775` with this machine's—,
    // and a file that tells what the user is up to is going to live there. Under the user's runtime
    // directory it would not be reachable, because it is already theirs and theirs alone; but the
    // fallback can be a temporary shared by everyone.
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

/// Eight hexadecimal digits.
///
/// The four in the example of the decision that sets this are 65,536 names, and with a creation that
/// fails if the name already exists a collision leaves a split without an identity. Thirty-two bits
/// cost the same.
fn generate_id() -> io::Result<String> {
    // The error is converted by hand and not with `?` so as not to have to enable for `getrandom`
    // the feature that brings that conversion: it is already compiled in the tree without it, and
    // asking for it would recompile it.
    let random = getrandom::u32().map_err(|failure| io::Error::other(failure.to_string()))?;
    Ok(format_id(random))
}

/// The formatting, apart from the draw, so it can be tested with given values.
///
/// The zero-padding is what makes them **always** eight digits: without it, a small number would
/// give a shorter name, and a test that looks at the length of a randomly drawn identifier would
/// almost never notice.
fn format_id(random: u32) -> String {
    format!("{random:08x}")
}

/// Where the splits' directories live.
///
/// The user's runtime directory is the first choice for what it is: theirs and theirs alone, in
/// memory, and the system empties it when the session closes. When it does not exist —macOS, or a
/// Linux session without a manager— it falls back to the system temporary, which is the only
/// alternative that exists everywhere.
fn base_dir() -> PathBuf {
    base_dir_from(std::env::var_os("XDG_RUNTIME_DIR").as_deref())
}

/// The decision, separated from reading the environment so it can be tested without touching it.
///
/// **A variable that is defined and empty is not a path**: letting it through would give a relative
/// path, and the directory would be created inside the application's working directory — without
/// failing, which is the worst thing it could do.
fn base_dir_from(value: Option<&OsStr>) -> PathBuf {
    match value {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => std::env::temp_dir(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directory_is_born_created_and_can_be_written_inside() {
        let panel = PanelDir::create().expect("create the panel directory");

        assert!(panel.path().is_dir(), "not a directory: {panel:?}");
        std::fs::write(panel.path().join(PANEL_FILE), b"{}")
            .expect("the producer has to be able to publish there");
    }

    #[test]
    fn the_path_ends_in_the_identifier() {
        let panel = PanelDir::create().expect("create the panel directory");

        let last = panel
            .path()
            .file_name()
            .expect("the path has a last segment")
            .to_string_lossy()
            .into_owned();

        assert_eq!(last, format!("ivy-{}", panel.id()));
    }

    #[test]
    fn when_dropped_the_directory_disappears() {
        let path = {
            let panel = PanelDir::create().expect("create the panel directory");
            panel.path().to_path_buf()
        };

        assert!(!path.exists(), "the directory remained: {}", path.display());
    }

    /// The contract file is written by the producer, not by this crate — and even so it takes it
    /// away on closing. Without this test, the deletion would pass the rest of the suite all the same
    /// and would leave a directory per split as soon as someone published for real.
    #[test]
    fn when_dropped_it_takes_the_contract_file_with_it() {
        let path = {
            let panel = PanelDir::create().expect("create the panel directory");
            std::fs::write(panel.path().join(PANEL_FILE), b"{\"lines\":[]}")
                .expect("publish a panel");
            panel.path().to_path_buf()
        };

        assert!(!path.exists(), "the directory remained: {}", path.display());
    }

    /// The endpoint is the contract's, exactly like the file, so closing a console takes it away and
    /// nobody is told the directory remains.
    ///
    /// **Without this the notice comes out on every console close**, which is what a second name in
    /// this directory costs: `remove_dir` is non-recursive on purpose, so one leftover of this
    /// crate's own is enough. The socket is written here as a plain file because what is being
    /// asserted is the deletion, not the binding — a bound socket is a directory entry all the same,
    /// and building one would drag the transport into the contract crate.
    #[test]
    fn when_dropped_it_takes_the_control_endpoint_with_it() {
        let path = {
            let panel = PanelDir::create().expect("create the panel directory");
            std::fs::write(panel.path().join(PANEL_FILE), b"{\"lines\":[]}")
                .expect("publish a panel");
            std::fs::write(panel.path().join(CONTROL_SOCKET), b"").expect("leave an endpoint");
            panel.path().to_path_buf()
        };

        assert!(!path.exists(), "the directory remained: {}", path.display());
    }

    /// The flip side of deletion without recursion: what does not belong to the contract is not
    /// touched, and that is why the directory is still there. It is the property that is wanted, not
    /// an oversight.
    #[test]
    fn a_file_that_is_not_the_contracts_leaves_the_directory_standing() {
        let path = {
            let panel = PanelDir::create().expect("create the panel directory");
            std::fs::write(panel.path().join("record.log"), b"producer's work")
                .expect("write something foreign to the contract");
            panel.path().to_path_buf()
        };

        // The fact is captured and cleanup happens **before** asserting: with the cleanup behind the
        // `assert`, a test failure would leave the directory there forever.
        let survived = path.join("record.log").is_file();
        std::fs::remove_dir_all(&path).expect("clean up what the test left on purpose");

        assert!(
            survived,
            "something that was not the contract's was deleted"
        );
    }

    #[test]
    fn two_directories_share_neither_identifier_nor_path() {
        let first = PanelDir::create().expect("create the first");
        let second = PanelDir::create().expect("create the second");

        assert_ne!(first.id(), second.id());
        assert_ne!(first.path(), second.path());
    }

    #[test]
    fn the_two_variables_are_the_contracts_and_point_to_the_directory() {
        let panel = PanelDir::create().expect("create the panel directory");

        let [(id_name, id_value), (dir_name, dir_value)] = panel.env();

        assert_eq!(id_name, "PANEL_ID");
        assert_eq!(dir_name, "PANEL_DIR");
        assert_eq!(id_value, OsStr::new(panel.id()));
        assert_eq!(dir_value, panel.path().as_os_str());
    }

    /// Uniqueness is not checked: it is obtained from creation failing. If this stopped failing, two
    /// splits could end up sharing a directory and nothing else in the crate would notice.
    #[test]
    fn creating_over_a_directory_that_already_exists_fails() {
        let panel = PanelDir::create().expect("create the panel directory");

        let again = create_dir(panel.path());

        assert!(
            again.is_err(),
            "creating over what already exists has to fail"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_cannot_be_read_by_anyone_else() {
        use std::os::unix::fs::PermissionsExt;

        let panel = PanelDir::create().expect("create the panel directory");

        let mode = panel
            .path()
            .metadata()
            .expect("read the permissions")
            .permissions()
            .mode()
            & 0o777;

        assert_eq!(
            mode, 0o700,
            "mode {mode:o}, and the panel is going to live there"
        );
    }

    #[test]
    fn the_real_identifier_has_eight_hexadecimal_digits() {
        let id = generate_id().expect("generate an identifier");

        assert_eq!(id.len(), 8, "not eight digits: {id}");
        assert!(
            id.chars()
                .all(|d| d.is_ascii_hexdigit() && !d.is_uppercase()),
            "not lowercase hexadecimal: {id}"
        );
    }

    /// With given values, and not just with a drawn one: looking at the length of a random identifier
    /// lets the missing zero-padding through, because it only shows with small numbers — one in
    /// sixteen.
    #[test]
    fn the_format_pads_with_zeros_up_to_eight() {
        assert_eq!(format_id(0), "00000000");
        assert_eq!(format_id(0xff), "000000ff");
        assert_eq!(format_id(0x0abcdef1), "0abcdef1");
        assert_eq!(format_id(u32::MAX), "ffffffff");
    }

    /// The case that does not fail and does the wrong thing: with the variable **defined and empty**
    /// the path would come out relative, and the panel directory would end up inside the working
    /// directory.
    #[test]
    fn an_empty_environment_variable_does_not_count_as_a_path() {
        assert_eq!(base_dir_from(Some(OsStr::new(""))), std::env::temp_dir());
    }

    #[test]
    fn without_a_runtime_directory_it_falls_back_to_the_system_temporary() {
        assert_eq!(base_dir_from(None), std::env::temp_dir());
    }

    #[test]
    fn with_a_runtime_directory_that_one_is_used() {
        assert_eq!(
            base_dir_from(Some(OsStr::new("/run/user/1000"))),
            PathBuf::from("/run/user/1000")
        );
    }

    #[test]
    fn deleting_what_is_no_longer_there_does_not_deserve_a_notice() {
        let path = {
            let panel = PanelDir::create().expect("create the panel directory");
            panel.path().to_path_buf()
        };

        assert!(
            remove(&path).is_none(),
            "it warned about a directory that was no longer there"
        );
    }

    #[test]
    fn what_is_left_undeleted_does_deserve_a_notice() {
        let panel = PanelDir::create().expect("create the panel directory");
        std::fs::write(panel.path().join("record.log"), b"from the producer")
            .expect("write something foreign to the contract");

        let warning = remove(panel.path());
        std::fs::remove_dir_all(panel.path()).expect("clean up what the test left on purpose");

        assert!(
            warning.is_some(),
            "the directory survived with something inside and nobody said so"
        );
    }
}
