//! Measures what it costs to read a `panel.json`: time per call and **memory peak while interpreting**.
//!
//! **It ties down the numbers the code asserts**: the peak and the time that
//! [`panel_contract::MAX_BYTES`] declares for a file at the ceiling, and the cost of the case that is
//! actually going to happen —the `panel.json` in the repo—. The previous version of that comment said
//! "10 MiB, about forty times the file" and nobody could check it again: it was 7× with realistic
//! lines and 95× in the worst case. A number written in the code with nothing to re-run it ages
//! badly, and this example exists so it does not happen again.
//!
//! **What it measured and is not here**, on purpose: the system calls per read, the cost of checking
//! that the bytes are text, and the comparison against what `serde` would derive. They are one-time
//! diagnostics —their answers already live in the comment of [`panel_contract::read`] and in the
//! design—, not numbers the code asserts, and the last one forced dragging along three duplicated
//! types of the contract that nobody would remember to update.
//!
//! ```text
//! cargo run --release --example read -p panel-contract
//! ```
//!
//! In debug the numbers come out several times worse; the ones that count are those of `--release`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

use panel_contract::{MAX_BYTES, read};

// ─── The memory counter ────────────────────────────────────────────────────────────────────────
//
// The peak cannot be obtained from outside the process: `/usr/bin/time` gives the resident maximum of
// the whole run, which here includes the test files. What is live at each instant has to be counted.

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct Counter;

unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Relaxed);
            add(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        if !moved.is_null() {
            REALLOCATIONS.fetch_add(1, Relaxed);
            LIVE.fetch_sub(layout.size(), Relaxed);
            add(new_size);
        }
        moved
    }
}

fn add(amount: usize) {
    let live = LIVE.fetch_add(amount, Relaxed) + amount;
    PEAK.fetch_max(live, Relaxed);
}

#[global_allocator]
static ALLOCATOR: Counter = Counter;

/// What a call cost, in memory.
struct Usage {
    peak: usize,
    allocations: usize,
    reallocations: usize,
}

/// Runs while measuring, and **returns what was produced, live**: the peak includes what the call leaves done.
fn while_measuring<T>(work: impl FnOnce() -> T) -> (T, Usage) {
    let baseline = LIVE.load(Relaxed);
    PEAK.store(baseline, Relaxed);
    ALLOCATIONS.store(0, Relaxed);
    REALLOCATIONS.store(0, Relaxed);

    let produced = work();

    let usage = Usage {
        peak: PEAK.load(Relaxed).saturating_sub(baseline),
        allocations: ALLOCATIONS.load(Relaxed),
        reallocations: REALLOCATIONS.load(Relaxed),
    };
    (produced, usage)
}

// ─── The measurements ──────────────────────────────────────────────────────────────────────────

fn main() {
    let bench = Bench::new();

    println!("== 1. The real `panel.json` of the repo ==\n");
    real_files(&bench);

    println!("\n== 2. A file right at the ceiling, in four shapes ==\n");
    println!(
        "With the same bytes, what changes the peak is how many values they carry: one node is"
    );
    println!("reserved for each.\n");
    for filler in fillers_at_the_cap() {
        at_the_cap(&bench, &filler);
    }

    println!("\n== 3. What reading the bytes reserves ==\n");
    allocations_of_a_read(&bench);
}

/// A temporary directory to write what is going to be read.
///
/// `PanelDir` is not used on purpose: it draws a random name and deletes on death, and here a stable
/// place is needed to rewrite the same file many times.
struct Bench {
    dir: PathBuf,
}

impl Bench {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("panel-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create the bench");
        Self { dir }
    }

    /// Publishes some content and returns the directory that `read` expects.
    fn publish(&self, contents: &str) -> &Path {
        std::fs::write(self.dir.join("panel.json"), contents).expect("publish");
        &self.dir
    }

    fn path(&self) -> PathBuf {
        self.dir.join("panel.json")
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The nine files in the repo, one by one.
fn real_files(bench: &Bench) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repo root")
        .to_path_buf();

    let mut real = Vec::new();
    collect_into(&root.join(".claude/work"), &mut real);
    collect_into(&root.join("prototypes"), &mut real);
    real.sort();

    if real.is_empty() {
        println!("  (no `panel.json` was found in the repo)");
        return;
    }

    println!(
        "  {:<46} {:>6} {:>4} {:>8} {:>9} {:>9} {:>7}",
        "file", "bytes", "lns", "µs read", "of those", "peak", "factor"
    );
    println!(
        "  {:<46} {:>6} {:>4} {:>8} {:>9} {:>9} {:>7}",
        "", "", "", "", "syscalls", "", ""
    );

    let mut total = 0.0;
    for path in &real {
        let contents = std::fs::read_to_string(path).expect("read the real file");
        let dir = bench.publish(&contents);

        // A cold pass before measuring: the first read pays for the directory cache.
        let _ = read(dir);

        const ROUNDS: u32 = 5000;
        let start = Instant::now();
        for _ in 0..ROUNDS {
            std::hint::black_box(read(std::hint::black_box(dir)).ok());
        }
        let micros = start.elapsed().as_secs_f64() * 1e6 / f64::from(ROUNDS);
        total += micros;

        // The same without interpreting: metadata + open + read_to_end, which is the irreducible floor.
        let file = bench.path();
        let start = Instant::now();
        for _ in 0..ROUNDS {
            std::hint::black_box(std::fs::metadata(&file).ok());
            std::hint::black_box(std::fs::read(&file).ok());
        }
        let io = start.elapsed().as_secs_f64() * 1e6 / f64::from(ROUNDS);

        let (panel, usage) =
            while_measuring(|| read(dir).expect("a real file from the repo is read"));

        println!(
            "  {:<46} {:>6} {:>4} {:>8.1} {:>9.1} {:>9} {:>6.1}×",
            short(path, &root),
            contents.len(),
            panel.lines.len(),
            micros,
            io,
            human(usage.peak),
            usage.peak as f64 / contents.len() as f64
        );
    }

    println!(
        "\n  Mean: {:.1} µs per read. At 60 frames per second, re-reading the nine in each one",
        total / real.len() as f64
    );
    println!("  would cost {:.2} ms of the frame's 16.7.", total / 1000.0);
}

fn collect_into(from: &Path, en: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_into(&path, en);
        } else if path.file_name().is_some_and(|n| n == "panel.json") {
            en.push(path);
        }
    }
}

fn short(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// A way to fill the ceiling, with its name.
struct Filler {
    name: &'static str,
    text: String,
}

/// Three ways to spend 256 KiB, from the most realistic to the two extremes.
fn fillers_at_the_cap() -> Vec<Filler> {
    // The shape a real panel has: objects with reference, text, mark, style and link.
    let real_line = r##"{"ref": "#12", "text": "extract Division from the window and leave it alone", "mark": "done", "style": "ok", "link": "https://github.com/mashware/ivy-history-private/pull/31"}"##;
    // The worst case by number of nodes: the smallest object that is still a line.
    let minimal_line = r#"{"text":"x"}"#;

    vec![
        Filler {
            name: "lines like those of a real panel",
            text: repeat(real_line),
        },
        Filler {
            name: "minimal contract objects (more nodes per byte)",
            text: repeat(minimal_line),
        },
        // The real worst case: the field does not even need to exist in the contract. It is enough
        // for the object to have **one** key for its map to reserve a whole tree node.
        Filler {
            name: "objects with any single key (the worst case)",
            text: repeat(r#"{"a":0}"#),
        },
        Filler {
            name: "a single giant line",
            text: {
                let padding = "x".repeat(MAX_BYTES as usize - 32);
                format!(r#"{{"lines":["{padding}"]}}"#)
            },
        },
    ]
}

/// Repeats an element until it just grazes the ceiling from below.
fn repeat(item: &str) -> String {
    let mut text = String::from(r#"{"lines":["#);
    let closing = "]}";
    while text.len() + item.len() + 1 + closing.len() < MAX_BYTES as usize {
        text.push_str(item);
        text.push(',');
    }
    text.pop();
    text.push_str(closing);
    text
}

fn at_the_cap(bench: &Bench, filler: &Filler) {
    let dir = bench.publish(&filler.text);
    let _ = read(dir);

    const ROUNDS: u32 = 50;
    let start = Instant::now();
    for _ in 0..ROUNDS {
        std::hint::black_box(read(std::hint::black_box(dir)).ok());
    }
    let millis = start.elapsed().as_secs_f64() * 1e3 / f64::from(ROUNDS);

    let before = LIVE.load(Relaxed);
    let (panel, usage) = while_measuring(|| read(dir).expect("at the ceiling it still reads"));
    // What **stays** live with the panel in hand: the peak is an instant, this is paid as long as
    // the split exists.
    let resident = LIVE.load(Relaxed).saturating_sub(before);

    // How much of that time is the disk and not the interpreter.
    let path = bench.path();
    let start = Instant::now();
    for _ in 0..ROUNDS {
        std::hint::black_box(std::fs::read(&path).expect("read the bytes"));
    }
    let io = start.elapsed().as_secs_f64() * 1e3 / f64::from(ROUNDS);

    println!("  {}", filler.name);
    println!(
        "    {} bytes · {} lines read",
        filler.text.len(),
        panel.lines.len()
    );
    println!(
        "    time     {millis:.2} ms  (of which {io:.2} ms is reading the bytes → {:.2} ms interpret)",
        millis - io
    );
    println!(
        "    peak     {}  = {:.1}× the file   ({} reservations, {} reallocations)",
        human(usage.peak),
        usage.peak as f64 / filler.text.len() as f64,
        usage.allocations,
        usage.reallocations
    );
    println!(
        "    stays    {} live with the panel in hand ({:.0} % of the peak)",
        human(resident),
        resident as f64 * 100.0 / usage.peak as f64
    );
    // Four splits **one after another in the same thread**, which is how it would happen: the peak
    // does not multiply by four, because the intermediate of each read dies before the next.
    let before = LIVE.load(Relaxed);
    PEAK.store(before, Relaxed);
    let four: Vec<_> = (0..4).map(|_| read(dir).expect("read")).collect();
    let peak_of_four = PEAK.load(Relaxed).saturating_sub(before);
    std::hint::black_box(&four);
    drop(four);

    println!(
        "    four splits in a row: {} of peak — not {}, because the intermediates do not coincide\n",
        human(peak_of_four),
        human(usage.peak * 4)
    );
}

/// The two ways of accumulating the read bytes, replicated and counted.
///
/// **It is a comparison, not a copy of `read_capped`**: what it ties down is *why* that function says
/// in advance how much it expects. Without the hint, `read_to_end` reads as much as fits in what is
/// reserved —starting at 32 bytes—, so each doubling of the buffer is one more system call: seven
/// reads for an 861 B panel and fifteen for one at the ceiling, counted with `strace`. With the hint
/// it is two, no matter what, and the final capacity stops being twice the file.
fn allocations_of_a_read(bench: &Bench) {
    println!(
        "  Accumulating the bytes without saying in advance how many are expected, versus saying it."
    );
    println!(
        "\n  {:<24} {:>9} {:>10} {:>16}",
        "file", "reservations", "realloc.", "final capacity"
    );

    for (name, contents) in [
        ("a real one (861 B)", "x".repeat(861)),
        ("16 KiB", "x".repeat(16 * 1024)),
        ("at the ceiling", "x".repeat(MAX_BYTES as usize)),
    ] {
        bench.publish(&contents);
        let path = bench.path();
        let (bytes, usage) = while_measuring(|| {
            let file = std::fs::File::open(&path).expect("open");
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .expect("read");
            bytes
        });

        // The same saying in advance how much is expected, bounded to the ceiling. The size costs
        // nothing: whoever reads has just asked for the metadata to check that it is a regular file.
        // And it decides nothing — the ceiling is still applied by the `take` and the check
        // afterwards, so a file that grows in between breaks nothing: the `Vec` grows on its own.
        let expected = std::fs::metadata(&path).expect("metadata").len();
        let (with_capacity, usage_with) = while_measuring(|| {
            let file = std::fs::File::open(&path).expect("open");
            let mut bytes = Vec::with_capacity(expected.min(MAX_BYTES) as usize + 1);
            file.take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .expect("read");
            bytes
        });

        println!(
            "  {:<24} {:>9} {:>10} {:>16}   →   with prior capacity: {} + {}, {}",
            name,
            usage.allocations,
            usage.reallocations,
            human(bytes.capacity()),
            usage_with.allocations,
            usage_with.reallocations,
            human(with_capacity.capacity())
        );
    }
}

fn human(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
