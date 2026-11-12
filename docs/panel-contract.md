# The panel document

What a producer — an agent, a CI run, a script — publishes about the console it works in, and what
Bellwether paints in that console's status panel, its header, its tab and the board. The app only
paints it: what the work means belongs to whoever does it.

The document started as Ivy's panel contract and keeps its rules. The reader is
`crates/panel-contract`; when this page and that crate disagree, the crate is right and this page is
the bug.

## Shape

```json
{
  "v": 1,
  "phase": "build",
  "attention": "wait",
  "header": true,
  "stale_after_minutes": 30,
  "actions": [{"label": "Run the tests", "send": "cargo test --workspace"}],
  "lines": [
    {"text": "Profile picker for the inbox", "style": "title"},
    "",
    {"text": "MRs", "style": "section", "summary": "2/4 merged", "key": "mrs"},
    {"text": "profile selector", "mark": "done", "ref": "#1", "link": "https://gitlab.com/acme/app/-/merge_requests/62"},
    {"text": "inbox and search", "mark": "current", "ref": "#3"}
  ]
}
```

A full two-block example, read by the test suite so it cannot drift:
[`docs/examples/blocks.json`](examples/blocks.json).

## Document fields

| Field | Required | What it does |
|---|---|---|
| `v` | recommended | Contract version |
| `updated_at` | no | What the producer says about when it published; read, not painted. The age comes from the file's date |
| `phase` | no | What is running **now**. Free text, painted literally on the identity row |
| `attention` | no | Whether this console claims you: `wait` (it is waiting for you), `block` (stuck on something else, not you), `done` (finished and nobody looked). Absent = claims nothing. The one field the app acts on: header symbol, tab, board and notifications. A `wait` is not shown while the console's program tells Herdr it is working: a program at work is not waiting for anyone, and a `wait` its producer forgot to clear must not keep claiming you |
| `attention_since` | no | When the current `attention` started, in seconds since the Unix epoch. The board orders who waits for you by it and warms the cards that have waited longest; it is never painted as a number. The MCP keeps it: it stamps it when `attention` changes and keeps it while it does not, replacing whatever the agent wrote. A producer writing the file declares it itself |
| `milestone` | no | The `id` of the milestone in the project's plan (see below) this console works on. The board hangs the console from it in the project's roadmap |
| `deliverable` | no | The `id` of the deliverable inside that milestone |
| `header` | no (`true`) | Whether the app adds its identity row (console and `phase`) |
| `stale_after_minutes` | no (30) | How much silence is legitimate before the panel says it has no news |
| `actions` | no | Buttons: `{"label", "send"}`, at most 8. The command is painted under its label and one click types it into the console. A `send` with a control, zero-width or direction-changing character, or over 512 characters, makes that button vanish rather than being cleaned |
| `lines` | **yes** | The body |

## Lines

A line is a string (a plain line; `""` is a gap) or an object:

| Field | What it does |
|---|---|
| `text` | The content. Cut to 512 characters when painted |
| `mark` | What state the line is in: `done` `current` `pending` `wait` `block` `info`. Painted as a symbol in its own column |
| `ref` | Short label in an aligned column — `#1`, `Now`, `Decision`. Not a number necessarily |
| `style` | How it is painted: `normal` `dim` `title` `accent` `ok` `warn` `error` `section` |
| `link` | An `http`/`https` address, painted as a compact label (`!42 ↗`, `#257 ↗`) that opens on click |
| `indent` | Indentation in cells, at most 64 |
| `key` | The name you give the line so `panel_patch` can address it. Never painted |
| `summary` | A block header's figure, pinned right. Only painted with `style: "section"`; see below |
| `collapsed` | `true` makes a block header start folded: only its title and summary show until the reader opens it. Only read with `style: "section"`. The producer decides it because only the producer knows which block is reference (merged MRs) and which is news; a reader's click wins over it |

A word outside the closed vocabularies (`attention`, `mark`, `style`) is dropped and the rest of the
line stands; over the MCP, the reply names what fell away. Unknown fields are ignored.

### Block headers — `style: "section"` and `summary`

A body is read in blocks separated by empty lines, and until headers a block had no name and no
count: you counted lines to know how much was left. A header line names the block that follows and
says how far along it is:

```
MRs ─────────────────────── 2/4 merged
✓ #1 profile selector                 !62
▸ #3 inbox and search                 !64
     └ pipeline ✓ · 0/1 approvals · 2 threads
Plan #3 ─────────────────────────────── 3/5
```

```json
{"text": "MRs", "style": "section", "summary": "2/4 merged", "key": "mrs"}
```

- **`text`** is the title, on the left. **`summary`** is the figure on the right — `2/4 merged`,
  `round 2 · 3 open`, `3/5` — at most **40 characters**; a longer one keeps its first forty. A rule
  fills the row between them.
- **When the column is narrow the title is cut, never the summary**: the figure is what the reader
  came for.
- On a header, `mark`, `ref`, `link` and `indent` are not painted. `key` works as on any line, so a
  producer keeps the count current with one `panel_patch` `replace` of the header line.
- `summary` on a line that is not a header is kept and ignored.
- **Why a style and a field of its own, not `ref`**: a reader that does not know headers paints `ref`
  as the left-hand label column, so a summary there would land before the title and widen the column
  of every line in the block. An unknown style is dropped and an unknown field ignored, so to an
  older reader a header is simply a line with its title as text — the panel never breaks.

## Rules

- **Whatever is not understood is ignored, field by field.** A document is refused only when it is not
  JSON, or carries no `lines`.
- **The body is free.** There are no fields for cost, context or progress: they are lines. The app
  understands marks, styles, alignment, headers and `attention`; the vocabulary belongs to the
  producer.
- **Publish before a long stretch, not after**, so a step that dies halfway is not shown as finished.
- **The panel says when it is old** (the file's date against `stale_after_minutes`) and when it could
  not be read, keeping the last good document on screen.

## The project plan — `plan.json`

A panel tells what one console is doing; a plan tells where the whole project is going. There is one
per git repository — its worktrees share it — in
`~/.local/state/bellwether/projects/<key>/plan.json`, where `<key>` is the absolute path of the main
checkout with `%` written `%25` and `/` written `%2F`. The board paints it, folded, on the project's
row.

```json
{
  "v": 1,
  "title": "Leave Quaderno",
  "source": "docs/roadmap.md",
  "milestones": [
    {"id": "H3", "title": "TaxRates", "done_when": "France on 1 Jan 2025 has the right rate",
     "depends_on": ["H0"],
     "items": [
       {"id": "catalog", "text": "Jurisdiction catalog", "mark": "done", "ref": "MT-16456"},
       {"id": "TAX-06", "text": "Manual load with approval", "mark": "pending"},
       "Backup sources"
     ]}
  ],
  "outside": ["Company certificate", {"text": "RSM", "mark": "pending"}]
}
```

- **`milestones` is required**; a milestone without an `id` is dropped, because no console could name
  it. At most 64 milestones and 128 items per milestone; the rest are cut, not refused.
- **An item** is a string or `{"text", "id", "mark", "ref", "link"}`; `mark` uses the line marks.
- **The plan states each deliverable's state; the board only counts.** A milestone's progress is its
  items marked `done` over all of them.
- **A console says where it works** with its panel's `milestone` and `deliverable`. A console of the
  project with neither shows as off the plan.
- Published with the MCP's `plan_set` (and read back with `plan_get`, removed with `plan_retire`), or
  by writing the file.

## Transports

| Transport | Where | For |
|---|---|---|
| **MCP** (`bellwether-panel-mcp`) | `panel_set`, `panel_patch`, `panel_get`, `panel_retire`; `plan_set`, `plan_get`, `plan_retire` for the project plan | An agent. Same verbs, arguments and replies as Ivy's channel, so `flow` publishes unchanged. Patching needs the console claimed with `panel_set` in the same session |
| **File** | `~/.local/state/bellwether/<terminal>/panel.json` (`BELLWETHER_STATE` overrides the base) | Anything else: a CI run, a script. Overwrite the whole file, ideally by writing a temporary file and renaming it |

The MCP writes the same file — it is the store the mods read, and what lets `panel_get` recover a
panel after the agent restarts. The console is `HC_TERMINAL_ID`, which Herdr gives every console.

For a console whose program does not report its own status to Herdr, the MCP also turns `attention`
into Herdr's agent status (OSC 7501): `wait` → `blocked` (question), `done` → `done`, `block` →
`working` with why it is stuck. Claude Code and Codex report their own, and theirs is never
overwritten.
