//! What `panel.json` says, and how it is read without anything it carries taking down the window.
//!
//! The names of the types and of their fields **are the contract's**, in English like it: whoever
//! compares this file with the specification does not have to translate anything along the way.

use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::dropped::{Dropped, Vocabulary};

/// The most that is read from a file.
///
/// It is not prudence: it bounds **what a file from outside can make get reserved when it is
/// interpreted**, and that is why it is applied **inside** the read —see [`read`]—, because checking
/// it afterwards would already have reserved.
///
/// **The multiplier depends on the shape of the file, not on its size**, and that is why there are
/// three numbers and not one. With exactly 256 KiB, measured in `--release` with
/// `cargo run --release --example read -p panel-contract`:
///
/// | Shape | Peak | Factor | Interpret |
/// |---|---|---|---|
/// | Lines of a real panel | 1.8 MiB | 7× | 2.5 ms |
/// | A single giant line | 769 KiB | 3× | 0.1 ms |
/// | Objects with one key, 32,766 of them — the worst case | **24.5 MiB** | **98×** | **22 ms** |
///
/// **Re-measured on 18 September 2026, when a line gained its optional key**, because a field added
/// to the shape of a line is exactly the thing that could move this table. **It does not move the
/// worst case, and the reason is worth keeping**: the peak follows the number of *values*, so the
/// worst shape is the one packing the most objects into the ceiling — which means the *fewest* keys
/// per object. A key on every line makes each object bigger and therefore fewer of them fit, which
/// walks away from this row rather than towards it — the example's own run of that shape, with two
/// keys per object instead of one, comes out at 68×. What the field costs is what it keeps once the
/// panel is in hand, not what interpreting it reserves.
///
/// **What the ceiling does NOT bound is the time.** The normal case does not come close: a realistic
/// panel is read in around **10 µs**, so re-reading the four of four splits would be a quarter of
/// 1 % of a frame. The one that bites is the pathological one —22 ms, a frame and a half—, and it is
/// enough that it is possible: whoever reads this from a painting loop **cannot re-read on every
/// frame**, it has to re-read only when the file has really changed.
///
/// And along the way it bounds the lines and their length, which is what makes it unnecessary to put a
/// separate ceiling on them.
pub const MAX_BYTES: u64 = 256 * 1024;

/// The most a line is indented.
///
/// It is the width of the widest panel in the mockup, in cells: more than that would not fit anyway,
/// and without a ceiling whoever paints would repeat as many spaces as a file from outside says.
pub const MAX_INDENT: u64 = 64;

/// How many buttons a producer may declare.
///
/// **Chosen against the panel, not against the file.** [`MAX_BYTES`] is 256 KiB and
/// `{"label":"a","send":"b"}` is 24 bytes, so a **legal** document declares about ten thousand
/// buttons — each one a frame and two pieces of text to lay out, which is the same tessellation cost
/// [`MAX_SEND`] is bounded against. At the panel's default width a button takes its label's row plus
/// one or two wrapped command rows, so eight of them take roughly half the rows a panel has on a
/// normal window: the most that can be declared while the body is still worth reading.
///
/// **Surplus entries are truncated, not rejected.** A document with nine buttons shows eight rather
/// than losing its panel, which is the same tolerance every other field of this contract gets.
pub const MAX_ACTIONS: usize = 8;

/// How long a button's command may be, in characters.
///
/// **A cost bound, and it is worth saying that plainly** — the first telling of this called it «the
/// painting ceiling restated», which is not checkable: what the painter bounds is *one row*, and a
/// 512-character command wraps over fifteen of them. What guarantees the command is *readable* is
/// that a button whose rows do not all fit is not pressable; what this number does is stop a legal
/// document from making the window lay out and wrap a quarter of a megabyte.
///
/// **Counted in characters and not bytes**, to match the unit the painter cuts on.
pub const MAX_SEND: usize = 512;

/// How long a block header's summary may be, in characters.
///
/// It shares one row with the header's title and a rule, in a column that can be thirty cells
/// wide, and it is the part that is never truncated when painted — the title gives way first — so
/// it is bounded here, where the document is read. **Cut, not refused**: a longer summary keeps its
/// first forty characters, the tolerance the rest of this contract gives a field it can still use.
/// Counted in characters, the unit the painter cuts on.
pub const MAX_SUMMARY: usize = 40;

/// How much silence is legitimate before warning, when the producer does not say.
pub const STALE_AFTER: Duration = Duration::from_secs(30 * 60);

/// What the producer publishes.
///
/// All fields may be absent except the lines: a panel without lines is not a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    /// Contract version. **Absent in seven of the nine files in the repo**, because it was added
    /// later: its absence is not an error.
    pub version: Option<u64>,
    /// What the producer says about when it published, as is, **as text and uninterpreted**. It is
    /// neither painted yet nor used to compute the age — the age comes from the file's own date, see
    /// [`Panel::written_at`], and `docs/04-panel-contract.md` records that this field is read and not
    /// painted.
    pub updated_at: Option<String>,
    /// What the producer is working on. Free text of its own, and painted literally: translating it
    /// to a color or an icon would be interpreting its vocabulary, and the vocabulary belongs to
    /// whoever does the work.
    pub phase: Option<String>,
    /// Whether the application adds its identity line at the top. By default, yes.
    pub header: bool,
    /// How much silence this producer tolerates before its panel is marked as old.
    pub stale_after: Duration,
    /// Whether this split **is calling for something**, and of what kind. Nothing = it is not calling.
    ///
    /// **It is the only field of the contract that the application interprets**, and that is
    /// deliberate: it answers the only question it needs in order to navigate —do I have to go there?—
    /// and from it come the panel's indicator and the one on its tab's label. The rest of the
    /// vocabulary —the marks, the styles, the phase— **is stored and painted**, because translating it
    /// would be the application deciding what someone else's work means.
    ///
    /// **It is not deduced from the body**: a line with [`Mark::Block`] is content of the summary, not
    /// a signal. Deducing it from there would be fragile in the silliest way — a "I waited for approval
    /// on Tuesday" would mark the tab forever.
    pub attention: Option<Attention>,
    /// Since when this console has been calling for what [`Panel::attention`] says, in **seconds
    /// since the Unix epoch**.
    ///
    /// **The producer declares it; nothing here measures it.** The board orders the consoles that
    /// wait for you by it and warms the ones that have waited longest, and a panel's own file date
    /// cannot stand in for it: a console that waits publishes nothing, so its file only says when it
    /// last spoke. The panel MCP stamps it whenever `attention` changes and keeps it while it does
    /// not, so an agent never writes it by hand; a producer that writes the file itself declares it.
    ///
    /// **A number and not a date string**, because this crate does not interpret dates (see
    /// [`Panel::written_at`]) and a reader in JavaScript multiplies it by a thousand. Without an
    /// `attention` it means nothing and is kept and ignored.
    pub attention_since: Option<u64>,
    /// The milestone of its project's plan this console works on, **by the `id` the plan gives
    /// it**. The board hangs the console from that milestone in the project's roadmap; an `id` the
    /// plan does not carry hangs it nowhere, and it is shown as off the plan.
    pub milestone: Option<String>,
    /// The deliverable inside [`Panel::milestone`] this console works on, by the plan's `id` for it.
    pub deliverable: Option<String>,
    /// The buttons the producer offers, **at the top level and not hanging off a line**.
    ///
    /// The reason is where they are painted. A line's place in the panel is decided by wrapping and
    /// by how much fits, and what does not fit is dropped from the bottom: a button hanging off a
    /// line would disappear in a short panel, or — worse — be drawn with its label visible and its
    /// command scrolled away, which is the one state the rule about seeing a command before it runs
    /// forbids. At the top level they join the block the application owns and the producer cannot
    /// suppress, which is the frontier [`Panel::attention`] already drew.
    pub actions: Vec<Action>,
    /// The body.
    pub lines: Vec<Line>,
    /// When the file was written, **which is where the age comes from**.
    ///
    /// It does not come from `updated_at`, and the reason is the contract's own: the producer
    /// **overwrites the whole file** on every publication, so the file's date *is* that of its last
    /// publication. This way there is a stale notice even when the producer does not write its mark
    /// —which is when it is most needed— and this crate does not need to interpret dates nor drag along
    /// a calendar.
    pub written_at: Option<SystemTime>,
}

/// A button the producer declares, **and the only shape in this contract that reaches a shell**.
///
/// Its two fields are private and its only constructor checks them, which is the whole of the
/// mechanism: there is then no path from JSON to a pseudoterminal that skips the gate, and a reviewer
/// checks that by reading one function instead of every call site. It is the same reasoning the
/// application's own paste filter states for staying private — *«an ungated door sitting beside the
/// gated one, equally reachable and better named, is one autocomplete away from being used»*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    label: String,
    send: String,
}

impl Action {
    /// The button, **or nothing if what it would send is not exactly what it would show**.
    ///
    /// Both strings must be made entirely of scalars that are painted as themselves: see
    /// [`is_shown_as_itself`]. `send` must also be at most [`MAX_SEND`] characters.
    ///
    /// **It rejects and never repairs, and that is the load-bearing decision.** The application
    /// paints the literal command under its label and one click sends it, so that painted text *is*
    /// the confirmation the security table demands. A filter that removed characters on the way out
    /// would mean the bytes that ran are not the bytes that were read. Rejecting here makes «painted
    /// and sent are the same string» true by construction rather than by care.
    ///
    /// A rejected button makes **that action** vanish, not the panel — the same tolerance per field
    /// the rest of this contract has.
    pub fn new(label: &str, send: &str) -> Option<Self> {
        // **An empty command is not a command.** The application appends the terminator, so a button
        // declaring nothing is one that presses a bare Enter into whatever the person had half
        // typed — which is the opposite of «the command is visible before it runs», with nothing
        // visible at all. An empty label is refused for the same reason: a button nobody can read.
        // **Blank is not empty, and both are refused.** The application appends the terminator, so a
        // button declaring nothing — or nothing but spaces — is one that presses a bare Enter into
        // whatever the person had half typed, with a command row that is on screen and says nothing.
        // «The command is visible before it runs» is not satisfied by a visible blank.
        if send.trim().is_empty() || label.trim().is_empty() || send.chars().count() > MAX_SEND {
            return None;
        }
        if !label.chars().all(is_shown_as_itself) || !send.chars().all(is_shown_as_itself) {
            return None;
        }
        Some(Self {
            label: label.to_owned(),
            send: send.to_owned(),
        })
    }

    /// What is painted on the button.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// What is painted under the label **and** written to the program, character for character.
    ///
    /// The terminator is not part of it: this text cannot carry a line break, so whether a button
    /// executes is a property of the application and not a character a producer can get wrong.
    pub fn send(&self) -> &str {
        &self.send
    }
}

/// Whether that scalar is painted as itself, which is what a button's text has to be made of.
///
/// **Two families are out, and the first is the one that has already been measured in this
/// repository.**
///
/// *Controls* ([`char::is_control`], which is Unicode's `Cc` — the C0 range, `DEL`, and `U+0080` to
/// `U+009F`) are handled by the kernel's line discipline **before the program sees them**: `0x03`
/// interrupts the foreground process, `0x1a` suspends the shell and leaves the pane looking dead,
/// `0x1c` dumps core and `0x04` closes the input. Wrapping the text protects from none of it. **A
/// line break and a tab are rejected with the rest**, unlike in a paste: a paste is a document a
/// person copied and its lines are the point, while a button is one command — a line break in it is a
/// button that runs two commands while painting one, and a tab in a line editor is the completion
/// key, so the same button would do something different depending on what is on disk.
///
/// *Zero-width and direction-changing scalars* are painted as nothing, or paint the rest backwards,
/// so the text a person reads is not the text that runs. **This is a list and lists are how these
/// checks rot**, so two things about it: it is a closed set of **scalars**, tested one by one below,
/// with none of the rescanning hazard that sinks a search for forbidden *sequences*; and it is
/// Unicode's `Cf` category as it stands, which grows — **plus the two line and paragraph
/// separators**, which are not `Cf` and paint as a line break all the same. What would replace the
/// whole list is a general-category table, and this crate carries two dependencies with a written
/// reason each — see its manifest.
///
/// **It is `pub` because two callers already need the same answer**, and a second list is how the
/// two drift: the action parser asks it to decide whether a command or a label is dropped
/// ([`Action`]), and the application asks it before painting text that arrived over the wire.
pub fn is_shown_as_itself(scalar: char) -> bool {
    if scalar.is_control() {
        return false;
    }
    !matches!(
        scalar,
        '\u{00ad}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061c}'
            | '\u{06dd}'
            | '\u{070f}'
            | '\u{0890}'..='\u{0891}'
            | '\u{08e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0000}'..='\u{e007f}'
    )
}

/// A line of the panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    pub text: String,
    /// What this line **is**. The application chooses what symbol it is painted with and how it is
    /// aligned.
    pub mark: Option<Mark>,
    /// Short label that is aligned in a column. It does not have to be a number.
    pub reference: Option<String>,
    /// How it is painted. **It can come alongside a mark**, and then the color is set by the style and
    /// the symbol by the mark: two of the nine files in the repo do it, and on purpose — it is how you
    /// say "this line is information, and its color *is* the data".
    pub style: Option<Style>,
    /// Manual indentation, in cells.
    pub indent: usize,
    /// The address the producer associates with this line. **It is read and not used yet**: painting
    /// it would show something different from what would be copied, because painting discards
    /// characters the string does have.
    pub link: Option<String>,
    /// The name its producer gives this line so it can be addressed later, **never painted**.
    ///
    /// It is deliberately not [`Line::reference`]: that one is painted, clipped to the width of its
    /// column, and legitimately repeated —`#1` twice in one panel is ordinary— so an address built on
    /// it would change when the text changes and would name two lines at once.
    ///
    /// **It carries no bound of its own**, unlike `indent` or an action's command: those are bounded
    /// against what painting them costs, and this is never painted. What holds it is [`MAX_BYTES`],
    /// the same ceiling that holds every other string of the document.
    ///
    /// **Two lines may carry the same key and both keep it.** Reading somebody else's document is
    /// not the moment to rewrite it, so the ambiguity is left exactly where the producer put it. What
    /// is done about it belongs to whoever addresses a line, and nothing does yet.
    pub key: Option<String>,
    /// What a block header says about its block, pinned to the right of its rule — `2/4 merged`,
    /// `round 2 · 3 open`. **Painted only on a [`Style::Section`] line**; on any other it is kept and
    /// ignored. Cut to [`MAX_SUMMARY`] characters when read.
    ///
    /// **Its own field, not [`Line::reference`]**: a reader that does not know headers paints `ref`
    /// as the label column on the left, so a summary there would land before the title and widen the
    /// column of every line in the block. A field it does not know it simply ignores.
    pub summary: Option<String>,
    /// Whether a block header **starts folded**: only its title and its summary show, and the
    /// block's lines stay hidden until the reader opens it. **Read only on a [`Style::Section`]
    /// line**; on any other it is kept and ignored.
    ///
    /// **The producer decides it because only the producer knows which block is reference and
    /// which is news** — a list of merged MRs against the question being asked right now. A reader
    /// that folds by the header's name would be interpreting the producer's vocabulary. What the
    /// reader does with a click afterwards is its own business: the click wins over this default.
    pub collapsed: bool,
}

/// What a line is. The six of the contract, not one more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Done,
    Current,
    Pending,
    Wait,
    Block,
    Info,
}

impl Mark {
    /// The six, and **the list the reader reads them from**: a word is a mark when some entry of this
    /// list is called that, and the channel's description is written from this same list. So reading
    /// and describing cannot disagree; a variant left out of it is neither read nor described.
    pub const ALL: [Self; 6] = [
        Self::Done,
        Self::Current,
        Self::Pending,
        Self::Wait,
        Self::Block,
        Self::Info,
    ];
}

/// What a split is calling for, when it is calling for something.
///
/// **It is ordered by the priority the contract declares**, not by the place where the variants are
/// written: when a tab has several splits, "what calls the most" is the maximum of their calls.
/// [`Attention::Wait`] is the greatest because **it is the only one that gets unblocked with you in
/// front of it right now**; a split blocked by a third party can wait five more minutes, and one that
/// finished and nobody has looked at, all day.
///
/// **The order is written, not derived, and the difference matters the day there is a fourth value.** A
/// derived `Ord` orders by the position of the variants in the file: whoever added one in the middle
/// would change the priority **silently**, without the compilation or any test failing. With the
/// `match` below, that fourth variant **does not compile** until someone decides what priority it has —
/// which is exactly the conversation that has to happen. And the variants can then be declared in
/// **the contract's order**, like those of [`Mark`], instead of the other way around just so the
/// derive works out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attention {
    /// **It is waiting for you**: a question, a permission, a decision.
    Wait,
    /// Stuck on something else, not you.
    Block,
    /// It finished and nobody looked.
    Done,
}

impl Attention {
    /// The three, for whoever has to iterate over them — a color table, a symbol one, a test — and the
    /// list the reader reads the word from, as [`Mark::ALL`] is.
    ///
    /// **It lives here, right next to `priority`, and not in every place that iterates over them.** A
    /// literal list repeated around admits a fourth variant **silently**, which is exactly what the
    /// `match` below prevents for the production tables: whoever adds one will not compile until
    /// touching `priority`, and then has this list three lines above.
    pub const ALL: [Self; 3] = [Self::Wait, Self::Block, Self::Done];

    /// How much it is calling, so two can be compared. Numbers with no more meaning than their order.
    fn priority(self) -> u8 {
        match self {
            Self::Done => 0,
            Self::Block => 1,
            Self::Wait => 2,
        }
    }
}

impl Ord for Attention {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority().cmp(&other.priority())
    }
}

impl PartialOrd for Attention {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// How a line is painted. The eight of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Dim,
    Title,
    Accent,
    Ok,
    Warn,
    Error,
    /// **A block's header**: its text on the left, a rule, and the line's [`Line::summary`] pinned
    /// to the right — `MRs ──────── 2/4 merged`. It names the block that follows, down to the next
    /// header or the end of the body, so the reader stops counting lines to know how much is left.
    ///
    /// **A style and not a mark**, because a mark says what state a line is in and is painted as a
    /// symbol in its own column; a header has no state and no symbol. And it degrades the way the
    /// contract already promises for any word it does not know: a reader older than this variant
    /// drops the style and paints the text as a normal line.
    Section,
}

impl Style {
    /// The eight, read and described from this list for the reason [`Mark::ALL`] gives.
    pub const ALL: [Self; 8] = [
        Self::Normal,
        Self::Dim,
        Self::Title,
        Self::Accent,
        Self::Ok,
        Self::Warn,
        Self::Error,
        Self::Section,
    ];
}

/// Why a panel could not be read.
///
/// Each variant is a different thing to tell the user: that its producer has not published yet looks
/// nothing like it having published something that cannot be understood.
#[derive(Debug)]
pub enum Problem {
    /// There is no file yet. The normal case before the first publication.
    Missing,
    /// There is something with that name, but it is not a file.
    ///
    /// **It matters because a named pipe would block the open**, and since reading happens inside the
    /// frame, that would be the window frozen. It is checked **before** opening, so it covers the
    /// oversight —a producer that creates there what it should not— and not a deliberate race: between
    /// the check and the open there is a gap, and whoever can write in that directory is the user
    /// themselves.
    NotRegular,
    /// It goes over the [`MAX_BYTES`] ceiling.
    TooBig,
    /// It could not be opened or read.
    Unreadable(std::io::Error),
    /// It was read, but it is not a panel: it is not JSON, or it carries no lines.
    Invalid(String),
    /// An instruction about a document was refused, with the sentence its sender reads.
    ///
    /// **It is the one variant whose `Display` is the sentence and nothing else**, and that is the
    /// difference it exists for: every other variant here says why a *document* could not be had, so
    /// wrapping it in «not understood (…)» tells the reader something true. A patch that named a key
    /// nobody carries was not misunderstood — it was understood exactly, and refused — and the
    /// contract publishes the words a producer gets back.
    ///
    /// **It widens this enum rather than adding a second one** because `serialize`'s ceiling refuses
    /// a patched document too: one caller reading two error vocabularies for one verb would have to
    /// translate between them to say anything at all.
    Refused(String),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "not published yet"),
            Self::NotRegular => write!(f, "there is something there that is not a file"),
            Self::TooBig => write!(f, "over {} KiB", MAX_BYTES / 1024),
            Self::Unreadable(failure) => write!(f, "could not be read ({failure})"),
            Self::Invalid(reason) => write!(f, "not understood ({reason})"),
            Self::Refused(why) => write!(f, "{why}"),
        }
    }
}

impl Panel {
    /// Whether what is shown is no longer trustworthy for being old.
    ///
    /// **The subtraction is signed on purpose.** A date in the future is not the rare case: a fast
    /// clock or a foreign filesystem is enough. With the naive subtraction of times that is a panic,
    /// and this path cannot panic over anything that comes from outside.
    pub fn is_stale_at(&self, now: SystemTime) -> bool {
        let Some(written) = self.written_at else {
            return false;
        };
        match now.duration_since(written) {
            Ok(silence) => silence > self.stale_after,
            // The file says it was written in the future: there is no silence to measure.
            Err(_) => false,
        }
    }
}

/// Reads a directory's panel **only if its file has changed** since the date it is given.
///
/// `Ok(None)` means "what you have is still current", and it is the answer that makes re-reading from
/// a painting loop cheap: asking for the date costs less than a microsecond, and interpreting can cost
/// a whole frame — see [`MAX_BYTES`].
///
/// **It lives here and not in the caller, and that boundary is the reason it exists.** Deciding whether
/// the file changed requires knowing **what the file is called**, and that name is the contract's:
/// whoever looked at it on their own would be writing it a second time, and would ask for the file's
/// data twice per read. The date to compare against comes from [`Panel::written_at`] of the previous
/// read.
///
/// Without a file the problem is returned, not `None`: "it disappeared" and "it is unchanged" are
/// different things for whoever paints.
pub fn read_if_newer(dir: &Path, seen: Option<SystemTime>) -> Result<Option<Panel>, Problem> {
    match std::fs::metadata(dir.join(crate::PANEL_FILE)) {
        Ok(metadata)
            if metadata
                .modified()
                .is_ok_and(|modified| Some(modified) == seen) =>
        {
            Ok(None)
        }
        // **Absence is resolved here and not by going back to [`read`]**, which is the most frequent
        // case of all: a console whose producer publishes nothing passes through here on every frame,
        // and letting it continue would ask for the file's data **twice** to reach the same
        // conclusion.
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => Err(Problem::Missing),
        _ => read(dir).map(Some),
    }
}

/// Reads the panel in a split's directory.
///
/// The file name is set by this crate, not the caller: it is part of the contract and lives in a
/// single place.
pub fn read(dir: &Path) -> Result<Panel, Problem> {
    let path = dir.join(crate::PANEL_FILE);

    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => {
            return Err(Problem::Missing);
        }
        Err(failure) => return Err(Problem::Unreadable(failure)),
    };
    if !metadata.is_file() {
        return Err(Problem::NotRegular);
    }

    let file = std::fs::File::open(&path).map_err(Problem::Unreadable)?;
    let bytes = read_capped(file, metadata.len()).map_err(Problem::Unreadable)?;
    // **The cap and the encoding are decided separately, and in this order.** Reading directly to
    // text tied the two things together: the cap's cut splits a multi-byte character —and here there
    // are accents and em dashes in every real file—, so a huge file came out as "could not be read"
    // and the "goes over the ceiling" path was almost never reached.
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    let text =
        String::from_utf8(bytes).map_err(|_| Problem::Invalid("not UTF-8 text".to_owned()))?;

    let mut panel = parse(&text)?;
    panel.written_at = metadata.modified().ok();
    Ok(panel)
}

/// Reads at most one byte more than the ceiling.
///
/// **How much of the file is read is decided here**, so `read` never holds a file it will refuse.
/// Looking at the size before opening seems cheaper, but it would be a second path that no test can
/// exercise —the first would always reject earlier— and on top of that it would leave out the race
/// that matters: the producer overwrites the whole file, so between looking and reading it may have
/// written a bigger one. **The refusal itself is [`parse`]'s**, shared with the delivered transport;
/// what this function bounds is the memory, not the verdict.
///
/// The extra byte is what lets "it takes up exactly the ceiling" be told apart from "it goes over". And
/// it is separate from the rest **so that how much is read can be asserted**: looking only at the
/// result of [`read`], a cap that bounded nothing would give the same error, because the check
/// afterwards would cover it up.
///
/// **`expected` is a size hint, not a check**, and the distinction is exactly the one the decision
/// above defends: nobody decides anything with that number, it only reserves room in one go. Without
/// it, `read_to_end` starts at 32 bytes and doubles: measured, **nine system calls for an 861-byte
/// panel and seventeen for one at the ceiling**, which on top of that leaves reserved twice what it
/// takes up. With the hint it is four and five, and no reallocation. It matters little on a local disk
/// and much on a network one, where each round trip is milliseconds inside the frame. If the producer
/// overwrites the file with a bigger one between the `metadata` and the read, the hint falls short and
/// `read_to_end` grows on its own: the ceiling is still set by the `take`.
fn read_capped(file: std::fs::File, expected: u64) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(expected.min(MAX_BYTES) as usize + 1);
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Turns a **delivered** document into a panel, with no file anywhere.
///
/// **It is the same parser the file goes through, and that is the whole of why it exists.** The
/// contract is the document, not the transport (**D17**): a second entry point with a parser of its
/// own would let the two transports drift — a field tolerated on one side and rejected on the other,
/// with nothing failing until a producer that worked over one stopped working over the other.
///
/// **What it does not do is stamp a date.** [`read`] takes [`Panel::written_at`] from the file's
/// own, and a delivered document has no such thing: whoever applies it stamps the moment it arrived,
/// which is what keeps a held panel able to go stale. Left here it would be a date this crate
/// invented.
pub fn parse_document(text: &str) -> Result<Panel, Problem> {
    parse(text)
}

/// The same reading as [`parse_document`], and **also what it let fall**.
///
/// The panel it returns is the one [`parse_document`] returns for the same text, always: what changes
/// is only that the losses are kept instead of forgotten. It is the channel's door, because a channel
/// has somebody to tell (**D58**); [`parse_document`] stays as it is for every reader that does not.
pub fn parse_document_noting(text: &str) -> Result<(Panel, Dropped), Problem> {
    let mut dropped = Dropped::default();
    let panel = parse_noting(text, Some(&mut dropped))?;
    Ok((panel, dropped))
}

/// Turns the text into a panel, tolerating per field.
///
/// **Tolerance is per field and not per document**, and that decides how it is read: a single field
/// that is not understood —a number where text went, a mark name that does not exist— is ignored and
/// its default value is used, instead of throwing away the whole file. Whoever publishes is an agent
/// writing JSON by hand: it will happen, and losing the complete panel over a field nobody looks at
/// would be the worst thing it could do.
///
/// There are only two ways for a file not to be valid: that it is not JSON, or that it carries no
/// lines.
///
/// **The ceiling lives here and not only in [`read`], because the two transports share this parser
/// (D17) and a ceiling on one of them is not a contract.** Putting it in `read` left
/// [`parse_document`] with no size control at all, so the channel accepted a document the file
/// refuses with [`Problem::TooBig`] — the drift the delivered transport exists to prevent. [`read`]
/// keeps its own check because it has to decide *before* turning a capped byte buffer into text: a
/// cap that cuts a multi-byte character would otherwise be reported as "not UTF-8" instead of "over
/// the ceiling".
fn parse(text: &str) -> Result<Panel, Problem> {
    parse_noting(text, None)
}

/// [`parse`], recording in `dropped`, when there is one, every vocabulary value it read as nothing.
///
/// **`None` is every reader but the channel's — the file's among them — and it records nothing at
/// all**: the file's reader runs inside the frame, where rendering a value nobody will read is work the
/// frame's cost would publish as the framework's (**D13**).
fn parse_noting(text: &str, mut dropped: Option<&mut Dropped>) -> Result<Panel, Problem> {
    if text.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    let raw: serde_json::Value =
        serde_json::from_str(text).map_err(|failure| Problem::Invalid(failure.to_string()))?;

    let lines = raw
        .get("lines")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Problem::Invalid("carries no \"lines\"".to_owned()))?;

    Ok(Panel {
        version: raw.get("v").and_then(serde_json::Value::as_u64),
        updated_at: string(&raw, "updated_at"),
        phase: string(&raw, "phase"),
        header: raw
            .get("header")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        stale_after: raw
            .get("stale_after_minutes")
            .and_then(serde_json::Value::as_u64)
            .map_or(STALE_AFTER, expiry_of),
        attention: vocabulary(
            &raw,
            "attention",
            attention_named,
            Vocabulary::Attention,
            dropped.as_deref_mut(),
        ),
        attention_since: raw.get("attention_since").and_then(serde_json::Value::as_u64),
        milestone: string(&raw, "milestone"),
        deliverable: string(&raw, "deliverable"),
        actions: raw
            .get("actions")
            .and_then(serde_json::Value::as_array)
            .map(|declared| {
                declared
                    .iter()
                    .filter_map(action)
                    .take(MAX_ACTIONS)
                    .collect()
            })
            .unwrap_or_default(),
        lines: lines
            .iter()
            .filter_map(|line| read_line_noting(line, dropped.as_deref_mut()))
            .collect(),
        written_at: None,
    })
}

/// How long a producer's number of minutes is, **read in one place because two readers reach it**.
///
/// A document says `stale_after_minutes` and a patch's `set` says it too, and the saturation is what
/// makes them the same value: `u64::MAX` minutes lands on `Duration::from_secs(u64::MAX)` rather
/// than wrapping. Written twice, the two would be one edit away from disagreeing — and a panel
/// published through one verb would go stale on a different clock from the same panel patched
/// through the other, which no test walks and nobody would see. The writer's `div_ceil` is the
/// inverse of exactly this function, and the comment over it names this one.
pub(crate) fn expiry_of(minutes: u64) -> Duration {
    Duration::from_secs(minutes.saturating_mul(60))
}

/// Turns an element of the list into a button, or **discards it**.
///
/// **A loose string is not an action**, unlike a line: a line has an obvious reading as bare text,
/// and there is no way to split one string into a label and a command without inventing one.
fn action(raw: &serde_json::Value) -> Option<Action> {
    Action::new(string_ref(raw, "label")?, string_ref(raw, "send")?)
}

/// Turns an element of the list into a line, or **discards it**.
///
/// An element that is neither a string nor an object —a number, a null— is not a line of anything:
/// painting it as a blank line would be inventing content out of garbage.
///
/// It records in `dropped`, when there is one, the mark or the style it read as nothing.
pub(crate) fn read_line_noting(
    raw: &serde_json::Value,
    mut dropped: Option<&mut Dropped>,
) -> Option<Line> {
    // A loose string is a normal line, and the empty string is a gap: the two cases are the same
    // here, and it is whoever paints that knows an empty text leaves a space.
    if let Some(text) = raw.as_str() {
        return Some(Line {
            text: text.to_owned(),
            ..Line::default()
        });
    }
    if !raw.is_object() {
        return None;
    }

    Some(Line {
        text: string(raw, "text").unwrap_or_default(),
        mark: vocabulary(raw, "mark", mark, Vocabulary::Mark, dropped.as_deref_mut()),
        reference: string(raw, "ref"),
        style: vocabulary(raw, "style", style, Vocabulary::Style, dropped),
        // Bounded **here**, which is the boundary with what comes from outside: whoever paints is
        // going to repeat that number of spaces, and an absurd value would be a panic from reserving
        // memory. The ceiling also prevents the conversion from silently truncating where the
        // machine's integer is smaller.
        indent: raw
            .get("indent")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .min(MAX_INDENT) as usize,
        link: string(raw, "link"),
        key: string(raw, "key"),
        summary: string_ref(raw, "summary").map(|summary| summary.chars().take(MAX_SUMMARY).collect()),
        collapsed: raw
            .get("collapsed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

/// Writes a panel back out as one line of JSON, or refuses it for being over [`MAX_BYTES`].
///
/// **This crate had no writer until this one**, and what it promises is not what a writer usually
/// promises. It cannot hand back the bytes its producer sent, because [`parse`] is lossy on purpose
/// —unknown fields dropped, `indent` clipped, `actions` truncated, a vocabulary word nobody knows
/// falling to nothing— and because a document that has been patched never had bytes of its own. What
/// it promises instead is a **round trip**:
///
/// ```text
/// parse_document(&serialize(&p)?) == Ok(Panel { written_at: None, ..p })
/// ```
///
/// for every panel [`parse`] can produce — so a document that has made one round trip never changes
/// again, and reading one back, editing it and publishing it again is stable.
///
/// **And it never grows a document**, which is a promise about bytes rather than about values and is
/// the reason nothing here is emitted unconditionally. An earlier shape of this function wrote
/// `"text"` always: a line published as `{"indent":1}` came back as `{"text":"","indent":1}`, ten
/// bytes longer, and a document of twenty thousand such lines —legal going in— was over the ceiling
/// coming out. A panel that paints perfectly would have answered *over 256 KiB* to a producer asking
/// what was on it. So the rule is: **emit no field the document did not carry**. Every other
/// difference between the input and this output shrinks it —whitespace gone, escapes resolved,
/// unknown fields dropped— which makes the bound hold in one direction only, and that is the
/// direction that matters.
///
/// **No newline anywhere**, because the reply that carries this is one line on a newline-framed
/// channel and this is the one reply that is interpolated raw rather than escaped. Every string here
/// goes through [`serde_json::Value::String`], so a newline inside somebody's text comes out as two
/// characters; the structure around them is this function's own.
///
/// **D48.**
pub fn serialize(panel: &Panel) -> Result<String, Problem> {
    let mut out = String::from("{");
    let mut comma = Comma::default();

    if let Some(version) = panel.version {
        comma.before(&mut out);
        out.push_str(&format!("\"v\":{version}"));
    }
    for (name, value) in [
        ("updated_at", panel.updated_at.as_deref()),
        ("phase", panel.phase.as_deref()),
        ("milestone", panel.milestone.as_deref()),
        ("deliverable", panel.deliverable.as_deref()),
    ] {
        if let Some(value) = value {
            comma.before(&mut out);
            out.push_str(&format!("\"{name}\":{}", quoted(value)));
        }
    }
    // **Only when it is false, and only when it differs**, and that is not brevity for its own sake:
    // after the parse there is no difference between «the producer did not say» and «the producer
    // said the default», so writing one would assert something this crate does not know. The price is
    // named in the contract document: a producer that published an explicit `"header": true` reads
    // back a document without it.
    if !panel.header {
        comma.before(&mut out);
        out.push_str("\"header\":false");
    }
    if panel.stale_after != STALE_AFTER {
        // **`div_ceil` and not a plain division.** [`expiry_of`] saturates, so a producer
        // asking for `u64::MAX` minutes lands on `Duration::from_secs(u64::MAX)` —fifteen seconds past
        // any multiple of sixty. Dividing gives a value whose multiplication does not saturate back
        // and the round trip breaks at exactly that point; rounding up overflows into the saturation
        // again and lands where it started. Everywhere [`expiry_of`] did not saturate — and both a
        // published document and a patch's `set` reach it — the seconds are an exact multiple of
        // sixty and the two agree.
        let minutes = panel.stale_after.as_secs().div_ceil(60);
        comma.before(&mut out);
        out.push_str(&format!("\"stale_after_minutes\":{minutes}"));
    }
    if let Some(attention) = panel.attention {
        comma.before(&mut out);
        out.push_str(&format!("\"attention\":\"{}\"", attention.name()));
    }
    if let Some(since) = panel.attention_since {
        comma.before(&mut out);
        out.push_str(&format!("\"attention_since\":{since}"));
    }
    if !panel.actions.is_empty() {
        comma.before(&mut out);
        out.push_str("\"actions\":[");
        for (at, action) in panel.actions.iter().enumerate() {
            if at > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"label\":{},\"send\":{}}}",
                quoted(action.label()),
                quoted(action.send())
            ));
        }
        out.push(']');
    }
    // **Always, even when there are none**, because a document with no `lines` is one of the only two
    // ways to be invalid: omitting it would emit something this crate's own parser refuses, which
    // breaks the round trip at the top rather than in a field.
    comma.before(&mut out);
    out.push_str("\"lines\":[");
    for (at, line) in panel.lines.iter().enumerate() {
        if at > 0 {
            out.push(',');
        }
        out.push_str(&serialize_line(line));
    }
    out.push_str("]}");

    if out.len() as u64 > MAX_BYTES {
        return Err(Problem::TooBig);
    }
    Ok(out)
}

/// Whether a comma is owed before the next field.
///
/// **A flag and not «is this the first»**, because the first field written is not a fixed one: every
/// field of a panel but `lines` is optional, so any of them can open the object and no position can
/// be relied on.
#[derive(Default)]
struct Comma(bool);

impl Comma {
    /// Writes the separator this field owes, if it owes one.
    fn before(&mut self, out: &mut String) {
        if self.0 {
            out.push(',');
        }
        self.0 = true;
    }
}

/// One line, **as a bare string where it can be**.
///
/// The bare form is what a producer writes for a gap or a plain sentence, and it is roughly half the
/// bytes of the object that says the same thing — which is what keeps a panel of gaps from doubling
/// against a ceiling the reply has to fit under. A line carrying a key is never bare, by
/// construction: a key is a field, and a field needs an object to live in.
fn serialize_line(line: &Line) -> String {
    let bare = line.mark.is_none()
        && line.reference.is_none()
        && line.style.is_none()
        && line.link.is_none()
        && line.key.is_none()
        && line.summary.is_none()
        && !line.collapsed
        && line.indent == 0;
    if bare {
        return quoted(&line.text);
    }

    let mut out = String::from("{");
    let mut comma = Comma::default();
    // Empty text is **absent** and not `""`: the parse cannot tell the two apart, and writing it is
    // the growth this function promises not to cause.
    if !line.text.is_empty() {
        comma.before(&mut out);
        out.push_str(&format!("\"text\":{}", quoted(&line.text)));
    }
    if let Some(mark) = line.mark {
        comma.before(&mut out);
        out.push_str(&format!("\"mark\":\"{}\"", mark.name()));
    }
    for (name, value) in [
        ("ref", line.reference.as_deref()),
        ("link", line.link.as_deref()),
        ("key", line.key.as_deref()),
        ("summary", line.summary.as_deref()),
    ] {
        if let Some(value) = value {
            comma.before(&mut out);
            out.push_str(&format!("\"{name}\":{}", quoted(value)));
        }
    }
    if let Some(style) = line.style {
        comma.before(&mut out);
        out.push_str(&format!("\"style\":\"{}\"", style.name()));
    }
    if line.indent > 0 {
        comma.before(&mut out);
        out.push_str(&format!("\"indent\":{}", line.indent));
    }
    // **Only when true**, for the reason `header` is written only when false: after the parse an
    // absent field and an explicit default are the same value, and writing it would grow the line.
    if line.collapsed {
        comma.before(&mut out);
        out.push_str("\"collapsed\":true");
    }
    out.push('}');
    out
}

/// A string as JSON, escaped by the library rather than by hand.
///
/// It is the same call [`crate`]'s neighbour makes when it builds a reply line: the one thing that
/// can go wrong in a document written by somebody else is a quote, a backslash or a newline inside
/// their text, and that is exactly what this buys.
fn quoted(text: &str) -> String {
    // **Over the borrowed string, not over an owned copy of it.** Going through
    // `Value::String(text.to_owned())` copied every string of the document once to build the value and
    // again to print it, on a path that runs inside the frame and whose worst case is a quarter of a
    // megabyte.
    serde_json::to_string(text).unwrap_or_else(|_| String::from("\"\""))
}

fn string(raw: &serde_json::Value, field: &str) -> Option<String> {
    string_ref(raw, field).map(str::to_owned)
}

/// The same, **without copying**: for the fields whose value is translated to an own type and thrown
/// away.
///
/// **It exists because this crate measures what it reserves.** A button's two strings are checked
/// before anything is kept, and copying them first would be reservations nobody uses, in a crate with
/// a whole module dedicated to counting reservations and a table of peaks in its documentation.
/// [`vocabulary`] reads the three closed vocabularies without copying for the same reason.
fn string_ref<'a>(raw: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    raw.get(field).and_then(serde_json::Value::as_str)
}

/// One of the three closed vocabularies read from `field`, and **the loss noted when it reads as
/// nothing**.
///
/// The value it returns is exactly `string_ref(raw, field).and_then(known)`: absent, null, a
/// non-string and an unknown word are all `None`. Only the last two are noted, and only when there is
/// somewhere to note them — an absent field and a null are what the producer said, not something that
/// fell away. It copies nothing on a value it could read, and renders one it could not only when a
/// record is there to keep it.
fn vocabulary<T>(
    raw: &serde_json::Value,
    field: &str,
    known: fn(&str) -> Option<T>,
    which: Vocabulary,
    dropped: Option<&mut Dropped>,
) -> Option<T> {
    let value = raw.get(field).filter(|value| !value.is_null())?;
    let read = value.as_str().and_then(known);
    if let (None, Some(dropped)) = (&read, dropped) {
        dropped.note(which, value);
    }
    read
}

pub(crate) fn attention_named(name: &str) -> Option<Attention> {
    Attention::ALL
        .into_iter()
        .find(|known| known.name() == name)
}

fn mark(name: &str) -> Option<Mark> {
    Mark::ALL.into_iter().find(|known| known.name() == name)
}

fn style(name: &str) -> Option<Style> {
    Style::ALL.into_iter().find(|known| known.name() == name)
}

// **The three vocabularies written back out, and read back in through the same words.** The readers
// above look each word up here, so a word cannot be spelled one way going in and another coming out:
// the round trip holds by construction, and each `match` below is exhaustive, so a new variant does
// not compile until it has a word. What no compiler and no test checks is that the variant is also
// in its `ALL`: a variant missing there is read as an unknown word — the panel paints without it and
// the channel's reply names it — and the tool's description leaves it out.

impl Attention {
    /// The word the contract spells it with.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Wait => "wait",
            Self::Block => "block",
            Self::Done => "done",
        }
    }
}

impl Mark {
    /// The word the contract spells it with.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Current => "current",
            Self::Pending => "pending",
            Self::Wait => "wait",
            Self::Block => "block",
            Self::Info => "info",
        }
    }
}

impl Style {
    /// The word the contract spells it with.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Dim => "dim",
            Self::Title => "title",
            Self::Accent => "accent",
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Section => "section",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PanelDir;

    /// Composed from the quirks of several real `panel.json` in the repo —it is not a copy of one—:
    /// **style and mark on the same line**, the version absent, and references of very different widths
    /// with an em dash.
    const ACTUAL: &str = r##"{
      "updated_at": "2026-08-11T22:00:00+02:00",
      "phase": "done",
      "header": true,
      "lines": [
        {"text": "F1.4: two splits side by side", "style": "title"},
        "",
        {"ref": "#1", "text": "extract Division from the window", "mark": "done",
         "link": "https://github.com/mashware/ivy-history-private/pull/31"},
        {"ref": "#1–#11", "text": "the knowledge", "mark": "wait"},
        {"ref": "Now", "text": "milestone closed", "mark": "info", "style": "ok"},
        "a loose line"
      ]
    }"##;

    fn panel_in(dir: &PanelDir, contents: &str) -> Result<Panel, Problem> {
        std::fs::write(dir.path().join(crate::PANEL_FILE), contents).expect("publish the panel");
        read(dir.path())
    }

    /// The two transports are one contract, and this is what holds them to it.
    ///
    /// **It asserts on the whole panel and not on a field of it**, because the drift this guards
    /// against is per field by construction: the parse tolerates each one separately, so a second
    /// entry point that diverged would diverge in whichever field nobody named here. The date is the
    /// one thing that legitimately differs — the file has one and a delivered document does not — so
    /// it is what the comparison sets aside, out loud.
    #[test]
    fn a_delivered_document_is_read_exactly_as_the_file_would_be() {
        let dir = PanelDir::create().expect("create the directory");
        let from_file = panel_in(&dir, ACTUAL).expect("read the panel");
        let delivered = parse_document(ACTUAL).expect("parse the delivered document");

        assert!(
            from_file.written_at.is_some(),
            "the file's read is what carries a date"
        );
        assert_eq!(delivered.written_at, None, "nothing delivered dates itself");
        assert_eq!(
            delivered,
            Panel {
                written_at: None,
                ..from_file
            }
        );
    }

    /// What is invalid over one transport is invalid over the other, and for the same two reasons:
    /// it is not JSON, or it carries no lines.
    #[test]
    fn a_delivered_document_is_refused_on_the_same_two_grounds() {
        assert!(matches!(
            parse_document("{\"lines\": 3}"),
            Err(Problem::Invalid(_))
        ));
        assert!(matches!(
            parse_document("not json"),
            Err(Problem::Invalid(_))
        ));
        assert!(parse_document("{\"lines\": []}").is_ok());
    }

    /// Every shape [`parse`] can produce, in one panel, for the round trip to walk.
    ///
    /// **Built by hand and not read from a file**, because what has to be covered is the parser's
    /// output space and not any particular producer's habits: all six marks, all seven styles,
    /// `MAX_ACTIONS` buttons, the indent at its ceiling, a key, the saturating expiry, a bare line, a
    /// gap, and text carrying the three characters that need escaping.
    fn every_shape() -> Panel {
        let mut lines = vec![
            Line {
                text: "a bare line".to_owned(),
                ..Line::default()
            },
            Line::default(),
            Line {
                text: "quote \" backslash \\ newline \n".to_owned(),
                key: Some("escapes".to_owned()),
                ..Line::default()
            },
            Line {
                text: "at the ceiling".to_owned(),
                indent: MAX_INDENT as usize,
                key: Some("indented".to_owned()),
                ..Line::default()
            },
            Line {
                key: Some("no text at all".to_owned()),
                ..Line::default()
            },
        ];
        for (at, mark) in [
            Mark::Done,
            Mark::Current,
            Mark::Pending,
            Mark::Wait,
            Mark::Block,
            Mark::Info,
        ]
        .into_iter()
        .enumerate()
        {
            lines.push(Line {
                text: format!("mark {at}"),
                mark: Some(mark),
                reference: Some(format!("#{at}")),
                link: Some("https://example.invalid/a b".to_owned()),
                ..Line::default()
            });
        }
        for (at, style) in [
            Style::Normal,
            Style::Dim,
            Style::Title,
            Style::Accent,
            Style::Ok,
            Style::Warn,
            Style::Error,
        ]
        .into_iter()
        .enumerate()
        {
            lines.push(Line {
                text: format!("style {at}"),
                style: Some(style),
                ..Line::default()
            });
        }
        lines.push(Line {
            text: "MRs".to_owned(),
            style: Some(Style::Section),
            summary: Some("2/4 merged".to_owned()),
            collapsed: true,
            ..Line::default()
        });
        Panel {
            version: None,
            updated_at: Some("2026-09-18T20:11:03+02:00".to_owned()),
            phase: Some("build".to_owned()),
            header: false,
            // The value the parse **saturates** on, which is the one point where writing the minutes
            // back with a plain division does not come home. See `serialize`.
            stale_after: Duration::from_secs(u64::MAX),
            attention: Some(Attention::Wait),
            attention_since: Some(1_794_474_068),
            milestone: Some("H3".to_owned()),
            deliverable: Some("TAX-06".to_owned()),
            // **No newline in the command, and that is not cosmetic.** `Action::new` refuses any
            // character the panel would not paint as itself, a line break included, so a `send`
            // carrying one is dropped and the list comes out empty — which left the writer's
            // `actions` branch reached by no test at all. Proved by deleting that branch with the
            // whole suite still green, which is exactly the disguise **D30** names.
            actions: (0..MAX_ACTIONS)
                .filter_map(|at| Action::new(&format!("button {at}"), &format!("echo {at}")))
                .collect(),
            lines,
            written_at: None,
        }
    }

    /// The property that replaces byte-fidelity, over everything the parser can build.
    ///
    /// **Asserted on the re-parsed panel and never on the emitted text.** A serializer that wrote the
    /// expiry in seconds instead of minutes would satisfy any assertion about the text it produced and
    /// still hand a producer a different document back, which is **D30**'s rule about asserting where
    /// a value is recorded instead of on the effect it has.
    #[test]
    fn a_document_that_made_one_round_trip_never_changes_again() {
        for panel in [
            every_shape(),
            parse_document(ACTUAL).expect("parse the composed document"),
            Panel {
                version: Some(1),
                lines: Vec::new(),
                ..every_shape()
            },
        ] {
            let written = serialize(&panel).expect("write the panel out");
            let read_back = parse_document(&written).expect("read back what was written");
            assert_eq!(read_back, panel, "the round trip changed the panel");

            let twice = serialize(&read_back).expect("write it out again");
            assert_eq!(twice, written, "a second round trip moved the bytes");
        }
    }

    /// **Writing a panel out never makes it bigger**, and the cases here are the tight ones.
    ///
    /// The bound is what keeps the channel's read able to answer for every document the file
    /// transport accepts: a source at the ceiling whose output grew would come back as *over 256 KiB* from a
    /// panel that paints perfectly. The sources are written **compact and minimal on purpose** — a
    /// pretty-printed document shrinks by so much that the assertion would pass however much this
    /// function added. Measured: **every one of these round-trips at zero slack**, byte for byte, so
    /// any field the writer started emitting unconditionally pushes all ten over their own length and
    /// not merely some of them.
    ///
    /// **`updated_at` and `actions` are covered by their absence rather than by a source of their
    /// own**, and that is deliberate: it is emitting a field the document did **not** carry that
    /// grows it, so a source where they are unset is exactly the one that catches an unconditional
    /// emission of either.
    #[test]
    fn writing_a_panel_out_never_makes_it_bigger() {
        for source in [
            r#"{"lines":[{"indent":1}]}"#,
            r#"{"lines":[{"key":"k"}]}"#,
            r#"{"lines":[{"mark":"done"}]}"#,
            r##"{"lines":[{"ref":"#1"}]}"##,
            r#"{"lines":[{"style":"dim"}]}"#,
            r#"{"lines":[{"link":"x"}]}"#,
            r#"{"lines":["","a"]}"#,
            r#"{"lines":[]}"#,
            r#"{"v":1,"phase":"p","attention":"wait","stale_after_minutes":5,"lines":[]}"#,
            r#"{"header":false,"lines":[{"text":"t","key":"k","indent":2}]}"#,
        ] {
            let panel = parse_document(source).expect("read the source");
            let written = serialize(&panel).expect("write it out");
            assert!(
                written.len() <= source.len(),
                "writing it out grew it: {} bytes in, {} out\n  in:  {source}\n  out: {written}",
                source.len(),
                written.len()
            );
        }
    }

    /// **No newline anywhere in what is written**, which the round trip cannot see.
    ///
    /// A raw newline emitted between two tokens re-parses perfectly, so the property above is blind to
    /// it — and this is the one reply of the control channel that is interpolated raw into a
    /// newline-framed line rather than escaped, so the byte that the round trip forgives is the byte
    /// that splits one reply into two on the socket.
    #[test]
    fn what_is_written_out_carries_no_newline() {
        let written = serialize(&every_shape()).expect("write the panel out");
        assert!(
            !written.contains('\n'),
            "a newline reached the written document: {written}"
        );
    }

    /// The ceiling applies to a panel **held in memory**, which until this delivery it never did.
    #[test]
    fn a_panel_too_big_to_write_is_refused_rather_than_written() {
        let fits = Panel {
            lines: (0..200)
                .map(|at| Line {
                    text: "x".repeat(1_000) + &at.to_string(),
                    ..Line::default()
                })
                .collect(),
            ..every_shape()
        };
        assert!(
            serialize(&fits).is_ok(),
            "a panel of some 200 KiB has to fit"
        );

        let does_not = Panel {
            lines: (0..300)
                .map(|at| Line {
                    text: "x".repeat(1_000) + &at.to_string(),
                    ..Line::default()
                })
                .collect(),
            ..every_shape()
        };
        assert!(matches!(serialize(&does_not), Err(Problem::TooBig)));
    }

    /// The ceiling is refused **at the byte after it, and accepted at it**.
    ///
    /// **The two panels of the test above sit nowhere near the boundary** — one at some 200 KiB, one
    /// at some 300 KiB — so turning `>` into `>=` leaves them both answering what they answered
    /// before. That is the disguise **D30** names by name: a margin where the defect and the fix give
    /// the same result. This one is built to land on the boundary exactly.
    #[test]
    fn a_panel_of_exactly_the_ceiling_is_written_and_one_byte_more_is_not() {
        // Everything a panel of one bare line costs around the text itself — `{"lines":[""]}`, the
        // quotes included — is what has to come off the ceiling to land exactly on it.
        let bare = |text: String| Panel {
            version: None,
            updated_at: None,
            phase: None,
            header: true,
            stale_after: STALE_AFTER,
            attention: None,
            attention_since: None,
            milestone: None,
            deliverable: None,
            actions: Vec::new(),
            lines: vec![Line {
                text,
                ..Line::default()
            }],
            written_at: None,
        };
        let around = serialize(&bare(String::new()))
            .expect("the empty shape fits")
            .len();
        let exactly = bare("x".repeat(MAX_BYTES as usize - around));

        let written = serialize(&exactly).expect("a panel of exactly the ceiling is written out");
        assert_eq!(
            written.len() as u64,
            MAX_BYTES,
            "the setup missed the boundary"
        );

        let one_more = bare("x".repeat(MAX_BYTES as usize - around + 1));
        assert!(matches!(serialize(&one_more), Err(Problem::TooBig)));
    }

    /// **A producer that uses no keys publishes exactly as it did before the field existed.**
    ///
    /// The criterion this binds is a promise of *no change*, which is the hardest kind to test and the
    /// easiest to believe on a green suite: every test written before this field existed passes
    /// whatever the field does, because none of them names it. So this asserts the two things that
    /// would actually move — that no line comes back carrying a key nobody wrote, and that a document
    /// with none is written out **byte for byte** as it arrived.
    #[test]
    fn a_producer_that_writes_no_keys_publishes_exactly_as_it_did_before() {
        let keyless = r##"{"phase":"build","lines":["a gap and a line",{"text":"t","mark":"done","ref":"#1"}]}"##;
        let panel = parse_document(keyless).expect("read the document");

        assert!(
            panel.lines.iter().all(|line| line.key.is_none()),
            "a line came back with a key its producer never wrote"
        );
        assert_eq!(
            serialize(&panel).expect("write it out"),
            keyless,
            "a document with no keys did not come back as the bytes it arrived as"
        );
    }

    /// A key is read and kept, **and a repeated one is left exactly where the producer put it**.
    ///
    /// Reading somebody else's document is not the moment to rewrite it: what this crate must not do
    /// is decide which of two lines an address means. The refusal belongs where the address is used,
    /// not where the document is read — and nothing uses one yet, which is what this guards.
    #[test]
    fn a_repeated_key_is_left_alone_on_both_lines() {
        let panel = parse_document(
            r#"{"lines":[{"text":"first","key":"mr-2"},{"text":"second","key":"mr-2"}]}"#,
        )
        .expect("read the document");

        let keys: Vec<_> = panel.lines.iter().map(|line| line.key.as_deref()).collect();
        assert_eq!(keys, vec![Some("mr-2"), Some("mr-2")]);
    }

    #[test]
    fn a_real_panel_is_read_whole_and_in_order() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(&dir, ACTUAL).expect("read the panel");

        assert_eq!(panel.lines.len(), 6);
        assert_eq!(panel.lines[0].style, Some(Style::Title));
        assert_eq!(panel.lines[1].text, "", "the empty string is a gap");
        assert_eq!(panel.lines[2].mark, Some(Mark::Done));
        assert_eq!(panel.lines[2].reference.as_deref(), Some("#1"));
        assert_eq!(panel.lines[3].reference.as_deref(), Some("#1–#11"));
        assert_eq!(panel.lines[5].text, "a loose line");
        assert_eq!(panel.phase.as_deref(), Some("done"));
    }

    /// The contract describes the style as something "for lines without a mark", but **two of the nine
    /// files in the repo combine them**, and on purpose: the mark says what the line is and the style
    /// says its color is the data. If this were read as mutually exclusive, one of the two would be
    /// lost.
    #[test]
    fn a_line_can_carry_a_mark_and_a_style_at_the_same_time() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(&dir, ACTUAL).expect("read the panel");

        let now_line = &panel.lines[4];
        assert_eq!(now_line.mark, Some(Mark::Info));
        assert_eq!(now_line.style, Some(Style::Ok));
    }

    #[test]
    fn the_version_can_be_absent() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(&dir, ACTUAL).expect("read the panel");

        assert_eq!(
            panel.version, None,
            "seven of the nine in the repo do not carry it"
        );
    }

    #[test]
    fn the_header_is_assumed_and_can_be_removed() {
        let dir = PanelDir::create().expect("create the directory");

        let unstated = panel_in(&dir, r#"{"lines":[]}"#).expect("read");
        assert!(unstated.header, "by default the application adds it");

        let removed = panel_in(&dir, r#"{"header":false,"lines":[]}"#).expect("read");
        assert!(!removed.header);
    }

    /// **Tolerance is per field, not per document.** Whoever publishes is an agent writing JSON by
    /// hand: a badly placed field will happen, and losing the whole panel over one that nobody looks
    /// at would be the worst thing that could be done with it.
    #[test]
    fn a_field_that_is_not_understood_does_not_throw_away_the_file() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(
            &dir,
            r#"{"v":"one","header":"yes","stale_after_minutes":-5,"attention":3,
                "lines":[{"text":"ok","mark":"made-up","style":42,"indent":"a lot"}]}"#,
        )
        .expect("a bad field does not invalidate the panel");

        assert_eq!(panel.version, None);
        assert_eq!(
            panel.attention, None,
            "a call that is not even a string has to fall into \"not calling\", not blow up"
        );
        assert!(
            panel.header,
            "a value that is not a boolean falls into the default"
        );
        assert_eq!(panel.stale_after, STALE_AFTER);
        assert_eq!(panel.lines.len(), 1);
        assert_eq!(panel.lines[0].text, "ok", "the line is kept");
        assert_eq!(
            panel.lines[0].mark, None,
            "a mark that does not exist is ignored"
        );
        assert_eq!(panel.lines[0].style, None);
        assert_eq!(panel.lines[0].indent, 0);
    }

    #[test]
    fn the_tolerated_silence_is_said_by_the_producer() {
        let dir = PanelDir::create().expect("create the directory");

        let its_own = panel_in(&dir, r#"{"stale_after_minutes":5,"lines":[]}"#).expect("read");
        assert_eq!(its_own.stale_after, Duration::from_secs(300));

        let by_default = panel_in(&dir, r#"{"lines":[]}"#).expect("read");
        assert_eq!(by_default.stale_after, STALE_AFTER);
    }

    #[test]
    fn without_lines_it_is_not_a_panel() {
        let dir = PanelDir::create().expect("create the directory");

        let failure =
            panel_in(&dir, r#"{"phase":"something"}"#).expect_err("without lines it is not valid");

        assert!(matches!(failure, Problem::Invalid(_)), "was {failure:?}");
    }

    /// A `lines` that is not a list **is not an empty panel**: if it were read as such, whoever paints
    /// would lose the last good panel and in its place would leave a gap, which is exactly what the
    /// distinction between "not understood" and "there is nothing" exists to avoid.
    #[test]
    fn lines_that_are_not_a_list_are_not_a_panel() {
        let dir = PanelDir::create().expect("create the directory");

        for text in [
            r#"{"lines":"just one"}"#,
            r#"{"lines":{"1":"one"}}"#,
            r#"{"lines":7}"#,
        ] {
            let failure = panel_in(&dir, text).expect_err("\"lines\" has to be a list");
            assert!(
                matches!(failure, Problem::Invalid(_)),
                "with {text} was {failure:?}"
            );
        }
    }

    /// Valid JSON but of another shape: the root has to be an object. Without this, looking for the
    /// lines in something that does not have them could end up taking the root itself for the panel's
    /// body.
    #[test]
    fn a_json_that_is_not_an_object_at_the_root_is_not_a_panel() {
        let dir = PanelDir::create().expect("create the directory");

        for text in [r#"["one","another"]"#, r#""just a string""#, "42", "null"] {
            let failure = panel_in(&dir, text).expect_err("the root has to be an object");
            assert!(
                matches!(failure, Problem::Invalid(_)),
                "with {text} was {failure:?}"
            );
        }
    }

    #[test]
    fn what_is_not_json_is_reported_without_breaking_anything() {
        let dir = PanelDir::create().expect("create the directory");

        // A half-written file is exactly this: JSON cut in half.
        let failure = panel_in(&dir, r#"{"lines":[{"text":"a hal"#).expect_err("not JSON");

        assert!(matches!(failure, Problem::Invalid(_)), "was {failure:?}");
    }

    /// **The zero-byte file is the intermediate instant of each publication**: the producer that
    /// overwrites opens to truncate and has not written anything yet. It has to come out "not
    /// understood" and not an empty panel, because an empty panel would erase the last good one right
    /// in the middle of each write.
    #[test]
    fn an_empty_file_is_a_half_write_and_not_a_panel_without_lines() {
        let dir = PanelDir::create().expect("create the directory");

        let failure = panel_in(&dir, "").expect_err("an empty file is not a panel");

        assert!(matches!(failure, Problem::Invalid(_)), "was {failure:?}");
    }

    #[test]
    fn not_published_yet_is_not_a_strange_error() {
        let dir = PanelDir::create().expect("create the directory");

        let failure = read(dir.path()).expect_err("there is no file");

        assert!(matches!(failure, Problem::Missing), "was {failure:?}");
    }

    /// "It has not published yet" and "there is something there I cannot read" are two different things
    /// to tell whoever looks, and only the first is normal. No permissions is the only thing that
    /// separates the two failure branches, and there are **two** branches: the one that looks at what
    /// is there and the one that opens it.
    #[cfg(unix)]
    #[test]
    fn without_permission_to_read_it_it_is_not_confused_with_not_having_published() {
        use std::os::unix::fs::PermissionsExt;

        let dir = PanelDir::create().expect("create the directory");
        let path = dir.path().join(crate::PANEL_FILE);
        std::fs::write(&path, r#"{"lines":[]}"#).expect("publish the panel");
        let mode = |path: &Path, mode| {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
                .expect("change the permissions")
        };

        // The permissions are restored right away, **before** asserting anything: with the restoration
        // behind the `assert`, a test failure would leave a directory its destructor can no longer
        // empty.
        mode(&path, 0o000);
        // Whoever runs the suite as administrator can read anyway, and then there is nothing to check
        // here. It is asked instead of assumed, which is cheaper than bringing in a library just to
        // look at the effective user.
        let unreadable = std::fs::File::open(&path).is_err();
        let from_file = read(dir.path());
        mode(&path, 0o600);

        mode(dir.path(), 0o000);
        let from_dir = read(dir.path());
        mode(dir.path(), 0o700);

        if unreadable {
            assert!(
                matches!(from_file, Err(Problem::Unreadable(_))),
                "the file without permissions came out as {from_file:?}"
            );
            assert!(
                matches!(from_dir, Err(Problem::Unreadable(_))),
                "the directory without permissions came out as {from_dir:?}"
            );
        }
    }

    /// **The silent readers record nothing**, and a record kept and thrown away reads back exactly the
    /// same panel, so the only place that shows is the allocator. Two documents of the same size, one
    /// whose marks the reader knows and one whose marks it drops, have to cost the same — both the most
    /// they ever hold at once and the number of reservations they make, because a value rendered and
    /// freed at once raises the second and not the first. That is the file's reader, inside the frame,
    /// paying nothing for a note nobody reads (**D13**).
    #[test]
    fn the_silent_readers_reserve_nothing_for_what_they_drop() {
        let lines = |mark: &str| vec![format!(r#"{{"text":"x","mark":"{mark}"}}"#); 500].join(",");
        let known = format!(r#"{{"lines":[{}]}}"#, lines("block"));
        let dropped = format!(r#"{{"lines":[{}]}}"#, lines("doing"));
        let ops = |mark: &str| -> serde_json::Value {
            serde_json::from_str(&format!(
                r#"[{{"op":"last","line":{{"text":"x","mark":"{mark}"}}}}]"#
            ))
            .expect("ops")
        };
        let (known_ops, dropped_ops) = (ops("block"), ops("doing"));

        let reading_known = while_counting(|| parse_document(&known).expect("a panel"));
        let reading_dropped = while_counting(|| parse_document(&dropped).expect("a panel"));
        let patching_known = while_counting(|| crate::parse_ops(&known_ops).expect("a patch"));
        let patching_dropped = while_counting(|| crate::parse_ops(&dropped_ops).expect("a patch"));

        assert_eq!(
            reading_dropped, reading_known,
            "the silent document reader kept a record: (peak, reservations)"
        );
        assert_eq!(
            patching_dropped, patching_known,
            "the silent patch reader kept a record: (peak, reservations)"
        );
    }

    /// **The same property as the one below, but over the real path.**
    ///
    /// The one below calls [`read_capped`] directly, so it only ties down the helper: replacing that
    /// call inside [`read`] with a `std::fs::read` —reserving the whole file, which is exactly what the
    /// ceiling exists to prevent— **the suite stayed green**, because the check afterwards returns
    /// [`Problem::TooBig`] all the same. Measured: the error does not tell the two paths apart, only
    /// the memory does.
    ///
    /// The file is eight times the ceiling, so reading it whole would reserve 2 MiB and the margin
    /// asserted against is 1 MiB: between what the honest path costs —around 512 KiB, the slack that
    /// `read_to_end`'s doubling leaves over 256 KiB— and what the dishonest one would cost there is a
    /// factor of four, which is plenty of room for this not to depend on the machine.
    #[test]
    fn reading_a_huge_file_does_not_reserve_what_it_takes_up() {
        let dir = PanelDir::create().expect("create the directory");
        let padding = "x".repeat(MAX_BYTES as usize * 8);
        let contents = format!(r#"{{"lines":["{padding}"]}}"#);
        std::fs::write(dir.path().join(crate::PANEL_FILE), &contents).expect("publish a huge one");
        drop(contents);

        let (failure, peak) = while_measuring(|| read(dir.path()));

        assert!(matches!(failure, Err(Problem::TooBig)), "was {failure:?}");
        assert!(
            peak < MAX_BYTES as usize * 4,
            "it reserved {peak} bytes reading a file that is rejected for being big"
        );
    }

    /// The property is **how much is read**, not what error comes out: looking only at the error of
    /// `read`, a cap that bounded nothing would give the same result because the check afterwards would
    /// cover it up.
    ///
    /// **And the size hint cannot turn into the cap.** It is passed the biggest lie that fits —that the
    /// file takes up nothing—: if some day someone reserves by the hint instead of reading up to the
    /// `take`, this case returns one byte and falls here.
    #[test]
    fn of_a_huge_file_only_the_first_bytes_are_read() {
        let dir = PanelDir::create().expect("create the directory");
        let path = dir.path().join(crate::PANEL_FILE);
        std::fs::write(&path, "x".repeat(MAX_BYTES as usize * 4)).expect("write a huge one");

        let read_back = read_capped(std::fs::File::open(&path).expect("open"), 0).expect("read");

        assert_eq!(
            read_back.len() as u64,
            MAX_BYTES + 1,
            "more than the ceiling and one byte was read"
        );
    }

    #[test]
    fn a_huge_file_is_not_read_whole() {
        let dir = PanelDir::create().expect("create the directory");
        let padding = "x".repeat(MAX_BYTES as usize + 1024);
        let too_big = format!(r#"{{"lines":["{padding}"]}}"#);

        let failure = panel_in(&dir, &too_big).expect_err("it goes over the ceiling");

        assert!(matches!(failure, Problem::TooBig), "was {failure:?}");
    }

    /// **With accents, which is the normal thing here**: every real file in the repo carries accents
    /// and em dashes. Reading directly to text tied the cap to the encoding, and the cut split a
    /// two-byte character: the huge file came out as "could not be read" and this path was never
    /// reached.
    #[test]
    fn a_huge_file_with_accents_is_still_too_big() {
        let dir = PanelDir::create().expect("create the directory");
        let padding = "ñ".repeat(MAX_BYTES as usize);

        let failure = panel_in(&dir, &format!(r#"{{"lines":["{padding}"]}}"#))
            .expect_err("it goes over the ceiling");

        assert!(matches!(failure, Problem::TooBig), "was {failure:?}");
    }

    /// A file that is not text is not "could not be read" —that sounds like permissions— but "not
    /// understood". And it is a real case: a write cut right between the two bytes of an accented
    /// letter.
    #[test]
    fn what_is_not_text_is_reported_as_what_it_is() {
        let dir = PanelDir::create().expect("create the directory");
        let path = dir.path().join(crate::PANEL_FILE);
        std::fs::write(&path, b"{\"lines\":[\"caf\xe9\"]}").expect("write raw bytes");

        let failure = read(dir.path()).expect_err("not UTF-8");

        assert!(matches!(failure, Problem::Invalid(_)), "was {failure:?}");
    }

    /// What is neither a string nor an object is not a line: painting it blank would be inventing
    /// content out of garbage.
    #[test]
    fn what_is_not_a_line_is_discarded_instead_of_painted_blank() {
        let dir = PanelDir::create().expect("create the directory");

        let panel = panel_in(&dir, r#"{"lines":[null,42,"good",{"text":"too"}]}"#)
            .expect("the good ones are read");

        assert_eq!(
            panel.lines.len(),
            2,
            "the null and the number are not lines"
        );
        assert_eq!(panel.lines[0].text, "good");
        assert_eq!(panel.lines[1].text, "too");
    }

    /// The time the producer tolerates is multiplied by sixty: an absurd number cannot overflow.
    #[test]
    fn an_absurd_tolerance_time_does_not_overflow() {
        let dir = PanelDir::create().expect("create the directory");

        let panel = panel_in(
            &dir,
            r#"{"stale_after_minutes":18446744073709551615,"lines":[]}"#,
        )
        .expect("read");

        assert_eq!(panel.stale_after, Duration::from_secs(u64::MAX));
    }

    /// The three words with which a producer says its split is calling for something.
    #[test]
    fn the_three_kinds_of_call_are_read() {
        let dir = PanelDir::create().expect("create the directory");

        for (written, expected) in [
            ("wait", Attention::Wait),
            ("block", Attention::Block),
            ("done", Attention::Done),
        ] {
            let panel = panel_in(&dir, &format!(r#"{{"attention":"{written}","lines":[]}}"#))
                .expect("read");

            assert_eq!(panel.attention, Some(expected), "with \"{written}\"");
        }
    }

    /// **The priority is the type's order, and what a tab's label shows depends on it.**
    ///
    /// It is asserted on the **maximum of a list**, which is how whoever accumulates is going to use
    /// it, and not on loose comparisons: with `a < b` in pairs, a badly declared order can keep passing
    /// if the pairs that are tested are not the ones that rule.
    ///
    /// **What this test CANNOT cover, and that is why the priority is written by hand**: a fourth kind
    /// of call that someone added tomorrow would not appear in any of these lists, so the test would
    /// stay green with the new priority undecided. What prevents it is that the `match` of `priority`
    /// stops compiling.
    #[test]
    fn what_calls_the_most_is_the_maximum() {
        use Attention::{Block, Done, Wait};

        assert_eq!([Done, Block, Wait].into_iter().max(), Some(Wait));
        assert_eq!([Block, Wait].into_iter().max(), Some(Wait));
        assert_eq!([Done, Block].into_iter().max(), Some(Block));
        assert_eq!([Done].into_iter().max(), Some(Done));
    }

    /// It is written by a third party by hand, so a word that does not exist is a matter of time — and
    /// the rest of its panel is not to blame.
    #[test]
    fn a_call_that_does_not_exist_is_read_as_if_nothing_had_been_said() {
        let dir = PanelDir::create().expect("create the directory");

        let panel = panel_in(
            &dir,
            r#"{"attention":"waiting","phase":"validate","lines":[{"text":"something"}]}"#,
        )
        .expect("read");

        assert_eq!(panel.attention, None);
        assert_eq!(
            panel.phase.as_deref(),
            Some("validate"),
            "a badly written field wiped out the rest of the panel"
        );
        assert_eq!(panel.lines.len(), 1, "and the body too");
    }

    /// **The body is not a signal, and this is the half of the milestone that lives in this crate.** A
    /// "I waited for approval on Tuesday" would be enough to leave a tab marked forever.
    ///
    /// **It is a regression test, not a coverage one, and it is worth knowing what makes it fail**:
    /// today there is no path that derives the call from the body, so the assertion holds by absence.
    /// It fails **when someone writes that path** — verified by adding to the read an `or_else` that
    /// looked for the first line with a wait or block mark. That is exactly the shortcut this milestone
    /// exists so that nobody takes.
    #[test]
    fn a_body_mark_is_not_a_call_from_the_split() {
        let dir = PanelDir::create().expect("create the directory");

        let panel = panel_in(
            &dir,
            r#"{"lines":[{"text":"I waited for approval on Tuesday","mark":"wait"},{"text":"and this","mark":"block"}]}"#,
        )
        .expect("read");

        assert_eq!(
            panel.lines[0].mark,
            Some(Mark::Wait),
            "the body does carry it"
        );
        assert_eq!(
            panel.attention, None,
            "the body produced a call for the whole split"
        );
    }

    /// The indentation comes from outside and whoever paints will repeat those spaces.
    #[test]
    fn an_absurd_indentation_is_bounded() {
        let dir = PanelDir::create().expect("create the directory");

        let panel = panel_in(
            &dir,
            r#"{"lines":[{"text":"t","indent":18446744073709551615}]}"#,
        )
        .expect("read");

        assert_eq!(panel.lines[0].indent, MAX_INDENT as usize);
    }

    /// A negative indentation is not "a lot of indentation": it is a badly written field and is worth
    /// its default value. The case exists because the number comes in unsigned, and reading it signed
    /// to then convert it would flip the value around and would indent up to the ceiling exactly when
    /// the producer was asking for the opposite.
    #[test]
    fn a_negative_indentation_does_not_flip_around() {
        let dir = PanelDir::create().expect("create the directory");

        let panel = panel_in(&dir, r#"{"lines":[{"text":"t","indent":-3}]}"#).expect("read");

        assert_eq!(panel.lines[0].indent, 0);
    }

    /// What is there might not be a file, and a named pipe would block the open: since reading happens
    /// inside the frame, that would be the window frozen.
    #[test]
    fn what_is_not_a_file_is_rejected_before_opening_it() {
        let dir = PanelDir::create().expect("create the directory");
        std::fs::create_dir(dir.path().join(crate::PANEL_FILE))
            .expect("put a directory where the file went");

        let failure = read(dir.path());
        // Cleanup happens **before** asserting: with the cleanup behind, a test failure would leave a
        // directory the destructor also cannot delete, because it is not empty.
        std::fs::remove_dir(dir.path().join(crate::PANEL_FILE)).expect("clean up");

        assert!(
            matches!(failure, Err(Problem::NotRegular)),
            "was {failure:?}"
        );
    }

    /// **The case the mechanism exists for, and until now was not tested.** The test above uses a
    /// directory, which is precisely the one that does **not** block: opening it fails and that is
    /// that. A named pipe does stay waiting for someone to write on the other side, and since reading
    /// happens inside the frame, that is the window frozen — forever, not for a while.
    ///
    /// If this test were to hang instead of fail, **that is the failure**: it means the regular-file
    /// check disappeared.
    #[cfg(unix)]
    #[test]
    fn a_named_pipe_is_rejected_instead_of_hanging_the_window() {
        let dir = PanelDir::create().expect("create the directory");
        let path = dir.path().join(crate::PANEL_FILE);
        let mkfifo = std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .is_ok_and(|status| status.success());
        assert!(mkfifo, "the named pipe could not be created");

        let failure = read(dir.path());
        std::fs::remove_file(&path).expect("clean up the pipe");

        assert!(
            matches!(failure, Err(Problem::NotRegular)),
            "was {failure:?}"
        );
    }

    /// **The flip side of requiring a file: a link to a normal file does work.** What is rejected is
    /// what would block the open, and a link blocks nothing. And it is not a contrived case: the
    /// producer that exists today publishes in its work directory, so linking the contract file there
    /// is the obvious way to appear in the panel without moving.
    #[cfg(unix)]
    #[test]
    fn a_link_to_a_normal_file_is_read_as_the_file_it_points_to() {
        let producer_dir = PanelDir::create().expect("create the producer's directory");
        let dir = PanelDir::create().expect("create the split's directory");
        let published = producer_dir.path().join(crate::PANEL_FILE);
        std::fs::write(&published, r#"{"lines":["from the other side"]}"#).expect("publish");
        std::os::unix::fs::symlink(&published, dir.path().join(crate::PANEL_FILE))
            .expect("link the contract file");

        let panel = read(dir.path()).expect("a link to a normal file is read");

        assert_eq!(panel.lines[0].text, "from the other side");
    }

    /// **The whole chain of the age, end to end.** The two tests below build the panel by hand, so
    /// none of them ties down the decision that matters: that the age comes from the file's date.
    /// Without this one, forgetting to store it would leave the suite green and **no panel would ever
    /// be marked old**.
    #[test]
    fn the_age_comes_from_the_date_of_the_read_file() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(&dir, r#"{"stale_after_minutes":1,"lines":[]}"#).expect("read");

        let written = panel.written_at.expect("the file's date has to come set");

        assert!(!panel.is_stale_at(written), "just written cannot be stale");
        assert!(
            panel.is_stale_at(written + Duration::from_secs(120)),
            "two minutes of silence with a cap of one is stale"
        );
    }

    /// The six marks and the eight styles, **all of them**. With only the ones an example file carries,
    /// deleting the arm of three marks and five styles left the suite green.
    #[test]
    fn the_six_marks_and_the_eight_styles_are_read() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(
            &dir,
            r#"{"lines":[
                {"text":"a","mark":"done"},{"text":"b","mark":"current"},
                {"text":"c","mark":"pending"},{"text":"d","mark":"wait"},
                {"text":"e","mark":"block"},{"text":"f","mark":"info"},
                {"text":"g","style":"normal"},{"text":"h","style":"dim"},
                {"text":"i","style":"title"},{"text":"j","style":"accent"},
                {"text":"k","style":"ok"},{"text":"l","style":"warn"},
                {"text":"m","style":"error"},{"text":"n","style":"section"}
            ]}"#,
        )
        .expect("read");

        let marks: Vec<Option<Mark>> = panel.lines[..6].iter().map(|l| l.mark).collect();
        assert_eq!(
            marks,
            [
                Some(Mark::Done),
                Some(Mark::Current),
                Some(Mark::Pending),
                Some(Mark::Wait),
                Some(Mark::Block),
                Some(Mark::Info)
            ]
        );

        let styles: Vec<Option<Style>> = panel.lines[6..].iter().map(|l| l.style).collect();
        assert_eq!(
            styles,
            [
                Some(Style::Normal),
                Some(Style::Dim),
                Some(Style::Title),
                Some(Style::Accent),
                Some(Style::Ok),
                Some(Style::Warn),
                Some(Style::Error),
                Some(Style::Section)
            ]
        );
    }

    /// **The example in the documentation is a document this reader understands**, read the way the
    /// file transport reads it, so the example a producer copies cannot drift from the contract.
    #[test]
    fn the_documented_two_block_example_reads_as_two_headers_with_their_summaries() {
        let panel = parse_document(include_str!("../../../docs/examples/blocks.json")).expect("read");

        let headers: Vec<(&str, Option<&str>)> = panel
            .lines
            .iter()
            .filter(|line| line.style == Some(Style::Section))
            .map(|line| (line.text.as_str(), line.summary.as_deref()))
            .collect();
        assert_eq!(headers, [("MRs", Some("2/4 merged")), ("Plan #3", Some("3/5"))]);
    }

    /// **A summary is cut, not refused, and cut on characters**: forty of them whatever their width
    /// in bytes, and the line keeps its header style and text. A summary that is not text is no
    /// summary, and the line still paints.
    #[test]
    fn a_summary_is_cut_to_its_ceiling_on_characters_and_a_non_text_one_is_ignored() {
        let long = "é".repeat(MAX_SUMMARY + 5);
        let panel = parse_document(&format!(
            r#"{{"lines":[{{"text":"MRs","style":"section","summary":"{long}"}},
                         {{"text":"Plan","style":"section","summary":7}}]}}"#
        ))
        .expect("read");

        assert_eq!(panel.lines[0].summary.as_deref(), Some("é".repeat(MAX_SUMMARY).as_str()));
        assert_eq!(panel.lines[0].style, Some(Style::Section));
        assert_eq!(panel.lines[1].summary, None);
        assert_eq!(panel.lines[1].text, "Plan");
    }

    /// **A header goes round and comes back the same**, summary included, so reading a panel back
    /// with `panel_get` and publishing it again keeps its headers — and a document without one is
    /// written exactly as before, which is what keeps every old producer's panel byte-for-byte.
    #[test]
    fn a_header_survives_the_round_trip_and_a_document_without_one_is_written_as_before() {
        let with = parse_document(r#"{"lines":[{"text":"MRs","style":"section","summary":"2/4 merged","key":"mrs"}]}"#)
            .expect("read");
        let written = serialize(&with).expect("write");
        assert_eq!(parse_document(&written).expect("read back"), with);
        assert!(written.contains(r#""summary":"2/4 merged""#), "{written}");

        let without = parse_document(r##"{"lines":["plain",{"text":"t","mark":"done","ref":"#1"}]}"##).expect("read");
        assert_eq!(
            serialize(&without).expect("write"),
            r##"{"lines":["plain",{"text":"t","mark":"done","ref":"#1"}]}"##
        );
    }

    /// **A reader older than headers sees a normal line**: an unknown style is dropped and an unknown
    /// field ignored, and the text stays. Simulated with a word this reader does not know in the
    /// style's place, which is exactly what `section` is to a reader built before it.
    #[test]
    fn a_header_style_a_reader_does_not_know_degrades_to_its_text() {
        let (panel, dropped) = parse_document_noting(
            r##"{"lines":[{"text":"MRs","style":"chapter","summary":"2/4 merged"},{"text":"#1","mark":"done"}]}"##,
        )
        .expect("read");

        assert_eq!(panel.lines[0].text, "MRs");
        assert_eq!(panel.lines[0].style, None);
        assert_eq!(panel.lines.len(), 2);
        assert!(dropped.sentence().is_some_and(|said| said.contains("chapter")));
    }

    /// The three fields that are only stored for someone to use later. Without this, "it is not read"
    /// and "it comes empty" are indistinguishable.
    #[test]
    fn the_version_the_producers_timestamp_and_the_link_are_read() {
        let dir = PanelDir::create().expect("create the directory");
        let panel = panel_in(
            &dir,
            r#"{"v":1,"updated_at":"2026-08-06T18:12:00+02:00",
                "lines":[{"text":"t","link":"https://example.test/1","indent":3}]}"#,
        )
        .expect("read");

        assert_eq!(panel.version, Some(1));
        assert_eq!(
            panel.updated_at.as_deref(),
            Some("2026-08-06T18:12:00+02:00")
        );
        assert_eq!(
            panel.lines[0].link.as_deref(),
            Some("https://example.test/1")
        );
        assert_eq!(panel.lines[0].indent, 3, "a valid indentation is kept");
    }

    /// The two exact boundaries, which is where a `>` too many or too few does not show.
    #[test]
    fn right_at_the_ceiling_it_is_accepted_and_one_byte_more_is_not() {
        let dir = PanelDir::create().expect("create the directory");
        let overhead = r#"{"lines":["WIDTH"]}"#.len() - "WIDTH".len();
        let exact_fit = "x".repeat(MAX_BYTES as usize - overhead);

        let fits =
            panel_in(&dir, &format!(r#"{{"lines":["{exact_fit}"]}}"#)).expect("it just fits");
        assert_eq!(fits.lines.len(), 1);

        let failure =
            panel_in(&dir, &format!(r#"{{"lines":["{exact_fit}x"]}}"#)).expect_err("one more, no");
        assert!(matches!(failure, Problem::TooBig), "was {failure:?}");
    }

    /// **The delivered document meets the same two boundaries as the file.** The channel used to
    /// accept a document the file refuses because the only size check lived in [`read`]; with the
    /// check in [`parse`] both transports of one contract decide the same way. Asserted on both
    /// boundaries and not just on the refusal, because a ceiling that refused everything would pass
    /// the second half alone.
    #[test]
    fn a_delivered_document_meets_the_ceiling_exactly_as_a_file_does() {
        let overhead = r#"{"lines":["WIDTH"]}"#.len() - "WIDTH".len();
        let exact_fit = "x".repeat(MAX_BYTES as usize - overhead);

        let fits = parse_document(&format!(r#"{{"lines":["{exact_fit}"]}}"#))
            .expect("it just fits over the channel too");
        assert_eq!(fits.lines.len(), 1);

        let failure =
            parse_document(&format!(r#"{{"lines":["{exact_fit}x"]}}"#)).expect_err("one more, no");
        assert!(matches!(failure, Problem::TooBig), "was {failure:?}");
    }

    #[test]
    fn silence_exactly_equal_to_the_cap_is_not_stale_yet() {
        let panel = Panel {
            stale_after: Duration::from_secs(60),
            written_at: Some(SystemTime::UNIX_EPOCH),
            ..empty_panel()
        };

        assert!(!panel.is_stale_at(SystemTime::UNIX_EPOCH + Duration::from_secs(60)));
        assert!(panel.is_stale_at(SystemTime::UNIX_EPOCH + Duration::from_secs(61)));
    }

    #[test]
    fn a_panel_silent_for_a_while_is_stale() {
        let panel = Panel {
            stale_after: Duration::from_secs(60),
            written_at: Some(SystemTime::UNIX_EPOCH),
            ..empty_panel()
        };

        assert!(panel.is_stale_at(SystemTime::UNIX_EPOCH + Duration::from_secs(120)));
        assert!(!panel.is_stale_at(SystemTime::UNIX_EPOCH + Duration::from_secs(30)));
    }

    /// A date in the future is not the rare case: a fast clock is enough. With the naive subtraction of
    /// times this would be a panic, and nothing that comes from outside can cause one.
    #[test]
    fn a_date_in_the_future_does_not_panic_nor_mark_stale() {
        let panel = Panel {
            stale_after: Duration::from_secs(60),
            written_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(7200)),
            ..empty_panel()
        };

        assert!(!panel.is_stale_at(SystemTime::UNIX_EPOCH + Duration::from_secs(60)));
    }

    #[test]
    fn without_knowing_when_it_was_written_there_is_no_staleness_to_measure() {
        assert!(!empty_panel().is_stale_at(SystemTime::now()));
    }

    /// The board orders who waits for you by this number, so it has to arrive as the producer
    /// wrote it, and a value of the wrong kind has to fall to nothing rather than to a guess.
    #[test]
    fn when_the_call_started_is_read_as_seconds_and_nothing_else() {
        let read = parse_document(r#"{"attention":"wait","attention_since":1794474068,"lines":[]}"#)
            .expect("a panel");
        assert_eq!(read.attention_since, Some(1_794_474_068));

        for wrong in [r#""1794474068""#, "-5", "1.5", "null"] {
            let text = format!(r#"{{"attention_since":{wrong},"lines":[]}}"#);
            let read = parse_document(&text).expect("the panel stands");
            assert_eq!(read.attention_since, None, "{wrong} was read as a moment");
        }
    }

    #[test]
    fn the_plan_a_console_works_on_is_read_as_the_producer_names_it() {
        let read = parse_document(r#"{"milestone":"H3","deliverable":"TAX-06","lines":[]}"#)
            .expect("a panel");
        assert_eq!(read.milestone.as_deref(), Some("H3"));
        assert_eq!(read.deliverable.as_deref(), Some("TAX-06"));
    }

    /// Only a JSON `true` folds a block: a producer writing `"yes"` gets an open block, which is the
    /// default and the safe side — a block hidden by a misspelling is news nobody reads.
    #[test]
    fn only_true_folds_a_block() {
        let read = parse_document(
            r#"{"lines":[
                {"text":"MRs","style":"section","collapsed":true},
                {"text":"Plan","style":"section","collapsed":"yes"},
                {"text":"Checks","style":"section"}
            ]}"#,
        )
        .expect("a panel");
        let folded: Vec<bool> = read.lines.iter().map(|line| line.collapsed).collect();
        assert_eq!(folded, [true, false, false]);
    }

    /// The writer promises never to grow a document, so an open block — the default — must not come
    /// back carrying `"collapsed":false`, and a plain line must stay a bare string.
    #[test]
    fn an_open_block_is_written_without_the_field() {
        let read = parse_document(r#"{"lines":[{"text":"MRs","style":"section","collapsed":false},"plain"]}"#)
            .expect("a panel");
        let written = serialize(&read).expect("it fits");
        assert_eq!(written, r#"{"lines":[{"text":"MRs","style":"section"},"plain"]}"#);
    }

    #[test]
    fn a_folded_block_and_the_call_start_survive_the_round_trip() {
        let text = r#"{"attention":"wait","attention_since":42,"milestone":"H3","deliverable":"TAX-06","lines":[{"text":"MRs","style":"section","collapsed":true}]}"#;
        let written = serialize(&parse_document(text).expect("a panel")).expect("it fits");
        let again: serde_json::Value = serde_json::from_str(&written).expect("JSON");
        assert_eq!(again["attention_since"], 42);
        assert_eq!(again["milestone"], "H3");
        assert_eq!(again["deliverable"], "TAX-06");
        assert_eq!(again["lines"][0]["collapsed"], true);
    }

    /// `collapsed` on a line that is not a header is kept and ignored, like `summary` — so the writer
    /// must still carry it, or a plain line with it would come back bare and the round trip breaks.
    #[test]
    fn a_fold_on_a_plain_line_is_kept_through_the_round_trip() {
        let read = parse_document(r#"{"lines":[{"text":"x","collapsed":true}]}"#).expect("a panel");
        let written = serialize(&read).expect("it fits");
        assert_eq!(parse_document(&written).expect("read back").lines, read.lines, "{written}");
    }

    fn empty_panel() -> Panel {
        Panel {
            version: None,
            updated_at: None,
            phase: None,
            header: true,
            stale_after: STALE_AFTER,
            attention: None,
            attention_since: None,
            milestone: None,
            deliverable: None,
            actions: Vec::new(),
            lines: Vec::new(),
            written_at: None,
        }
    }

    // ─── Counting what is reserved ──────────────────────────────────────────────────────────────
    //
    // It exists for a single test, and there was no cheaper way: the property "the whole file is not
    // read" **is not visible in the result** —the error is the same whether a quarter of a meg or ten
    // is reserved—, so either the memory is measured or it is not tied down. From outside the process
    // it is no good either: the resident maximum the system gives is that of the whole run, and here
    // dozens of tests run at once.
    //
    // **The count is per thread, on purpose.** The tests run in parallel, and a shared counter would
    // make what one reserves get blamed on another: the failure would be intermittent, which is the
    // worst kind of test. With one thread per test, each measures its own.

    /// What is live and the maximum it ever was, **in this thread**.
    ///
    /// They are initialized without reserving —hence the `const`—: a lazy value would reserve memory
    /// inside the allocator the first time it is looked at, which is a call to itself.
    mod counting {
        use std::cell::Cell;

        thread_local! {
            pub static LIVE: Cell<usize> = const { Cell::new(0) };
            pub static PEAK: Cell<usize> = const { Cell::new(0) };
            pub static RESERVATIONS: Cell<usize> = const { Cell::new(0) };
        }
    }

    struct Counter;

    /// Adds to or subtracts from what is live and keeps the maximum.
    ///
    /// `try_with` and not `with`: when a thread is torn down, its thread-locals are gone, and there
    /// memory is still freed. With `with` that would be a panic inside the allocator.
    fn record(delta: isize) {
        let _ = counting::LIVE.try_with(|live| {
            let current = live.get().saturating_add_signed(delta);
            live.set(current);
            let _ = counting::PEAK.try_with(|peak| peak.set(peak.get().max(current)));
        });
    }

    unsafe impl std::alloc::GlobalAlloc for Counter {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            let pointer = unsafe { std::alloc::System.alloc(layout) };
            if !pointer.is_null() {
                record(layout.size() as isize);
                let _ = counting::RESERVATIONS.try_with(|made| made.set(made.get() + 1));
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
            record(-(layout.size() as isize));
            unsafe { std::alloc::System.dealloc(pointer, layout) }
        }

        unsafe fn realloc(
            &self,
            pointer: *mut u8,
            layout: std::alloc::Layout,
            new_size: usize,
        ) -> *mut u8 {
            let moved = unsafe { std::alloc::System.realloc(pointer, layout, new_size) };
            if !moved.is_null() {
                record(new_size as isize - layout.size() as isize);
            }
            moved
        }
    }

    #[global_allocator]
    static ALLOCATOR: Counter = Counter;

    /// Runs the work and returns the most extra it ever had live and **how many reservations it made**
    /// — dropping what it produced first, so neither number depends on the result outliving the call.
    fn while_counting<T>(work: impl FnOnce() -> T) -> (usize, usize) {
        let made = counting::RESERVATIONS.get();
        let (produced, peak) = while_measuring(work);
        let reservations = counting::RESERVATIONS.get() - made;
        drop(produced);
        (peak, reservations)
    }

    /// Runs the work and returns, besides what it produced, **how much extra it ever had live**.
    fn while_measuring<T>(work: impl FnOnce() -> T) -> (T, usize) {
        let baseline = counting::LIVE.get();
        counting::PEAK.set(baseline);

        let produced = work();

        (produced, counting::PEAK.get().saturating_sub(baseline))
    }
}

/// What a button may carry, and what happens to one that carries something else.
///
/// **The subject is the gate, and the gate is [`Action::new`]** — the parser's only way in. So these
/// go through a whole document rather than through the constructor: an assertion on the constructor
/// alone would stay green if the parser ever stopped calling it.
#[cfg(test)]
mod buttons {
    use super::*;

    fn actions_of(declared: &str) -> Vec<Action> {
        let document = format!(r#"{{"actions": {declared}, "lines": ["x"]}}"#);
        parse(&document)
            .expect("the document carries lines")
            .actions
    }

    /// A button carrying one scalar, named so a failure says which one.
    ///
    /// **The scalar goes in ESCAPED, and that is not a convenience of the test.** A raw control
    /// character is not legal inside a JSON string, so the document a real producer writes carries
    /// `\u0003` and it is `serde_json` that turns it back into the character before this crate ever
    /// sees it. Writing the raw byte here would make the document invalid and the test would pass on
    /// the wrong failure — which is what the first version of these did.
    fn with(poison: char) -> Vec<Action> {
        let escaped = format!("\\u{:04x}", poison as u32);
        let declared = format!(r#"[{{"label": "Hidden", "send": "ls{escaped}x"}}]"#);
        actions_of(&declared)
    }

    #[test]
    fn a_button_carries_what_it_shows_and_what_it_sends() {
        let actions = actions_of(r#"[{"label": "Run tests", "send": "npm test"}]"#);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label(), "Run tests");
        assert_eq!(actions[0].send(), "npm test");
    }

    /// **The whole point of the type, asserted on the effect**: what a producer writes has to survive
    /// the read unchanged, because the application paints that string and sends that string.
    #[test]
    fn what_survives_the_read_is_what_the_producer_wrote() {
        let actions = actions_of(r#"[{"label": "Deploy", "send": "make deploy ENV=staging"}]"#);

        assert_eq!(actions[0].send(), "make deploy ENV=staging");
    }

    /// A line break is the character that makes a shell execute, so a button carrying one would run
    /// two commands while painting one.
    #[test]
    fn a_command_carrying_a_line_break_is_dropped() {
        assert!(with('\n').is_empty());
        assert!(with('\r').is_empty());
    }

    /// In a line editor the tab is the completion key, so the same button would do something
    /// different depending on what is on disk.
    #[test]
    fn a_command_carrying_a_tab_is_dropped() {
        assert!(with('\t').is_empty());
    }

    /// The four the kernel's line discipline acts on before any program sees them. Each one is named
    /// rather than looped over, so a failure says which.
    #[test]
    fn a_command_carrying_a_control_the_kernel_acts_on_is_dropped() {
        assert!(with('\u{03}').is_empty(), "interrupt survived");
        assert!(with('\u{1a}').is_empty(), "suspend survived");
        assert!(with('\u{1c}').is_empty(), "dump survived");
        assert!(with('\u{04}').is_empty(), "end of input survived");
    }

    /// `char::is_control` covers `U+0080`–`U+009F` too, and a terminal that parses eight-bit controls
    /// reads `U+009B` as a control sequence introducer.
    #[test]
    fn a_command_carrying_an_eight_bit_control_is_dropped() {
        assert!(with('\u{9b}').is_empty());
    }

    /// Painted as nothing and sent all the same: bytes the eye never saw reaching the shell.
    #[test]
    fn a_command_carrying_a_zero_width_scalar_is_dropped() {
        for invisible in ['\u{200b}', '\u{feff}', '\u{ad}', '\u{2060}'] {
            assert!(
                with(invisible).is_empty(),
                "{invisible:?} survived the read"
            );
        }
    }

    /// The text a person reads would not be the text that runs.
    #[test]
    fn a_command_carrying_a_direction_override_is_dropped() {
        for reversing in ['\u{202e}', '\u{2066}'] {
            assert!(
                with(reversing).is_empty(),
                "{reversing:?} survived the read"
            );
        }
    }

    /// **The list is a Unicode release behind and that is how it rots**: the Arabic pound and piastre
    /// marks have been `Cf` since Unicode 14 and were missing, so a command carrying one was painted
    /// as nothing and sent all the same. The line and paragraph separators are not `Cf` and paint as
    /// a break, which is the same defect by another route.
    #[test]
    fn the_newest_format_characters_and_the_separators_are_not_shown_as_themselves() {
        for hidden in ['\u{0890}', '\u{0891}', '\u{2028}', '\u{2029}'] {
            assert!(
                !is_shown_as_itself(hidden),
                "{hidden:?} would be painted as nothing"
            );
        }
    }

    /// The label is painted, so the same rule applies to it — and a line break there would reflow the
    /// strip under a button whose place was worked out for one row.
    #[test]
    fn a_label_carrying_a_line_break_drops_the_button_too() {
        let declared = "[{\"label\": \"Two\\nrows\", \"send\": \"ls\"}]";

        assert!(actions_of(declared).is_empty());
    }

    #[test]
    fn a_command_longer_than_the_ceiling_is_dropped() {
        let inside = "a".repeat(MAX_SEND);
        let over = "a".repeat(MAX_SEND + 1);

        assert_eq!(
            actions_of(&format!(r#"[{{"label": "x", "send": "{inside}"}}]"#)).len(),
            1
        );
        assert!(actions_of(&format!(r#"[{{"label": "x", "send": "{over}"}}]"#)).is_empty());
    }

    /// **Counted in characters, not bytes**, which is the unit the painter cuts on. Its own test
    /// because the two differ only on text that is not ASCII, and that is exactly where a byte count
    /// would refuse a command that fits.
    #[test]
    fn the_ceiling_counts_characters_and_not_bytes() {
        let japanese = "日".repeat(MAX_SEND);

        assert!(
            japanese.len() > MAX_SEND,
            "the case needs more bytes than characters"
        );
        assert_eq!(
            actions_of(&format!(r#"[{{"label": "x", "send": "{japanese}"}}]"#)).len(),
            1
        );
    }

    /// **Truncated and not rejected**: a document with one button too many shows the ones that fit
    /// rather than losing its panel.
    #[test]
    fn more_buttons_than_fit_are_truncated_and_the_panel_survives() {
        let declared: Vec<String> = (0..MAX_ACTIONS + 3)
            .map(|nth| format!(r#"{{"label": "b{nth}", "send": "echo {nth}"}}"#))
            .collect();
        let actions = actions_of(&format!("[{}]", declared.join(",")));

        assert_eq!(actions.len(), MAX_ACTIONS);
        assert_eq!(actions[0].label(), "b0");
    }

    /// One bad button takes neither the good ones nor the body with it: the same tolerance per field
    /// the rest of this contract has.
    #[test]
    fn a_dropped_button_leaves_the_others_and_the_body_standing() {
        let declared = r#"[{"label": "Good", "send": "ls"},
                {"label": "Bad", "send": "ls\u0003x"},
                {"label": "Also good", "send": "pwd"}]"#;
        let document = format!(r#"{{"actions": {declared}, "lines": ["the body survives"]}}"#);
        let panel = parse(&document).expect("the document carries lines");

        assert_eq!(panel.actions.len(), 2);
        assert_eq!(panel.actions[1].label(), "Also good");
        assert_eq!(panel.lines[0].text, "the body survives");
    }

    /// An element that cannot be read as a button is dropped, and a loose string is one of them:
    /// there is no way to split it into a label and a command without inventing one.
    #[test]
    fn an_element_that_is_not_a_button_is_dropped() {
        assert!(actions_of(r#"["just a string"]"#).is_empty());
        assert!(actions_of(r#"[{"label": "no command"}]"#).is_empty());
        assert!(actions_of(r#"[{"send": "no label"}]"#).is_empty());
        assert!(actions_of("[7, null]").is_empty());
    }

    /// **`lines` stays the only required field.** A strip of buttons with no summary is not a panel,
    /// and loosening that would turn this contract's single validity rule into two.
    #[test]
    fn a_document_with_buttons_and_no_body_is_still_invalid() {
        let document = r#"{"actions": [{"label": "Run", "send": "ls"}]}"#;

        assert!(matches!(parse(document), Err(Problem::Invalid(_))));
    }

    #[test]
    fn a_document_that_declares_no_buttons_has_none() {
        assert!(
            parse(r#"{"lines": ["x"]}"#)
                .expect("valid")
                .actions
                .is_empty()
        );
    }
}

/// The scalars the list of invisibles names, one test per branch.
///
/// **Six of its twenty-six branches were covered and twenty were not**, which a mutation found by
/// deleting four of them and watching the suite stay green. A list nothing names is a list somebody
/// trims by accident — the same shape as a guard list with no floor, one level down.
#[cfg(test)]
mod every_invisible {
    use super::*;

    /// One scalar out of every branch of the predicate, in the order they are written there.
    const ONE_PER_BRANCH: [char; 20] = [
        '\u{00ad}',
        '\u{0600}',
        '\u{0605}',
        '\u{061c}',
        '\u{06dd}',
        '\u{070f}',
        '\u{08e2}',
        '\u{180e}',
        '\u{200b}',
        '\u{200f}',
        '\u{202a}',
        '\u{202e}',
        '\u{2060}',
        '\u{2064}',
        '\u{2066}',
        '\u{206f}',
        '\u{feff}',
        '\u{fff9}',
        '\u{fffb}',
        '\u{110bd}',
    ];

    /// The four that live past the basic planes, where a byte-counting walk is likeliest to slip.
    const BEYOND_THE_BASIC_PLANE: [char; 4] = ['\u{110cd}', '\u{13430}', '\u{1bca0}', '\u{1d173}'];

    #[test]
    fn every_branch_of_the_list_refuses_its_scalar() {
        for invisible in ONE_PER_BRANCH.into_iter().chain(BEYOND_THE_BASIC_PLANE) {
            assert!(
                Action::new("x", &format!("ls{invisible}x")).is_none(),
                "{invisible:?} (U+{:04X}) is in the list and was accepted",
                invisible as u32
            );
        }
    }

    /// **The other direction, and it is the one that keeps the list from swallowing the alphabet.**
    /// A predicate that refused everything would pass the test above and every button would vanish.
    #[test]
    fn ordinary_text_is_not_refused() {
        for ordinary in [
            "ls -la",
            "cargo test",
            "grep 日本語 notes.txt",
            "echo «ok»",
            "cd ~/x",
        ] {
            assert!(
                Action::new("x", ordinary).is_some(),
                "{ordinary:?} was refused and it is ordinary text"
            );
        }
    }

    #[test]
    fn a_command_of_only_spaces_is_refused() {
        assert!(Action::new("x", "   ").is_none());
        assert!(Action::new("   ", "ls").is_none());
    }
}
