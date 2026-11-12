# Reporting to Bellwether — a guide for AI agents

You are an agent working in a [Herdr](https://herdr.dev) console. Bellwether shows a person what
every console is doing: a **status panel** beside each console, a **header** on each pane, and a
**board** with every console of every space. It paints **only what you publish**. It never reads
your screen, your files or your transcript to guess how you are doing. If you publish nothing, the
person sees nothing.

This page tells you what to publish and when. The full specification is
[`panel-contract.md`](panel-contract.md); when the two disagree, the contract wins.

## How to publish

Use the `bellwether` MCP server when your harness has it:

| Tool | What it does |
|---|---|
| `panel_set {source, document}` | Publish this console's whole panel. Call it first: it makes you the console's producer |
| `panel_patch {ops}` | Change part of it: replace, insert or delete lines by `key`, or `set` `phase`, `attention`, `header` or `stale_after_minutes` |
| `panel_get {}` | Read back what is on screen, also after a restart. Do this before republishing |
| `panel_retire {}` | Remove your panel |
| `plan_set {document, repo?}` | Publish your project's roadmap (see below) |
| `plan_get {repo?}` / `plan_retire {repo?}` | Read it back / remove it |

Tool names may come namespaced (`mcp__bellwether__panel_set`) and may be deferred: look for a tool
whose name ends in `panel_set`.

Without the MCP, pipe the document to `bellwether-panel-mcp publish`, which publishes it as
`panel_set` would. Or write the file yourself: `~/.local/state/bellwether/<HC_TERMINAL_ID>/panel.json`.
Overwrite it whole: write a temporary file and rename it over the old one.

## The panel

```json
{
  "v": 1,
  "phase": "build",
  "attention": "wait",
  "milestone": "H3",
  "deliverable": "TAX-06",
  "actions": [{"label": "Merge !74", "send": "glab mr merge 74"}],
  "lines": [
    {"text": "MT-16576 · Tax rate changes with approval", "style": "title"},
    "",
    {"text": "MRs", "style": "section", "summary": "1/2 merged", "collapsed": true, "key": "mrs"},
    {"text": "exclusive until dates", "mark": "done", "ref": "#1", "link": "https://gitlab.com/acme/app/-/merge_requests/73"},
    {"text": "rate batches with approval", "mark": "current", "ref": "#2"},
    "",
    {"text": "!74 is green. Merge it?", "mark": "wait", "ref": "Decision", "key": "decision"}
  ]
}
```

What each part becomes on screen:

- **The `title` line** is what the person reads first. It heads your card on the board and appears in
  the pane header after the project name: `Claude Code  tax-platform: MT-16576 · Tax rate changes…`.
  Make it the task, not your status.
- **`phase`** is what you are doing *now*, in a word or two (`investigate`, `build`, `review`,
  `ship`). It is painted as you write it.
- **`attention`** is the only field that moves things:
  - `wait`: **you need the person**: a question, a permission, a decision. Your card goes to *Waiting
    for you*, the pane header shows `?`, and the person gets a notification.
  - `block`: you are stuck on something that is not the person, like a broken environment or another
    team. The card goes to *Stuck*.
  - `done`: you finished and nobody has looked yet. The card goes to *Finished*.
  - Leave it out while you simply work. Clear it (`set attention null`) as soon as it stops being
    true.
- **Lines** are free text with a `mark` (`done` `current` `pending` `wait` `block` `info`), an optional
  `ref` column (`#1`, `Now`, `Decision`), a `style` and a `link`. Put the question you are asking on
  a line marked `wait`: it is what the person reads on the board.
- **Block headers** (`style: "section"`) name the block below them and carry a `summary` of at most
  40 characters (`2/4 merged`). Add `"collapsed": true` to reference blocks the person rarely needs
  open, such as merged MRs. Keep news and decisions open.
- **`actions`** are buttons. One click types `send` into **your** console, exactly as written. Offer
  the answers to your own question here (`Merge !74`, `Not yet`). They are commands run in the
  person's shell, so never put anything in them you would not run yourself.
- **`key`** names a line so `panel_patch` can update it later without republishing everything.

**Do not write `attention_since`.** The MCP stamps when the current `attention` started and keeps
it while `attention` stays the same. The board uses it to order who has waited longest and to warm
up their cards.

**Publish before a long stretch, not after**, so a step that dies halfway is not shown as finished.
A panel silent for longer than `stale_after_minutes` (30 by default) is shown as having no news,
unless it is waiting for the person.

## The roadmap

A project, meaning a git repository and all its worktrees, can have one roadmap. The board shows
it, folded, on the project's row. Publish it with `plan_set` when you read the project's plan, and
again when a deliverable changes state:

```json
{
  "title": "Leave Quaderno",
  "source": "docs/roadmap.md",
  "milestones": [
    {"id": "H3", "title": "TaxRates", "done_when": "France on 1 Jan 2025 has the right rate",
     "depends_on": ["H0"],
     "items": [
       {"id": "catalog", "text": "Jurisdiction catalog", "mark": "done", "ref": "MT-16456"},
       {"id": "TAX-06", "text": "Manual load with approval", "mark": "current", "ref": "MT-16576"}
     ]}
  ],
  "outside": [{"text": "Company certificate", "mark": "pending"}]
}
```

- Copy each state **as the source states it**. The board counts what is marked `done` and invents
  nothing. Mark `current` only what someone is really working on.
- Give each milestone and each deliverable a stable `id`, such as the ticket or the requirement code.
  Then say on your panel which ones you work on, with `milestone` and `deliverable`, and your card
  hangs from that deliverable in the roadmap. If your work is not in the plan, leave both out: the
  board shows you as *off the plan*, which is the truth.
- `outside` lists work the plan waits on that is not code, such as a certificate, a contract or a
  decision.

## What the board does with it

Columns, first match wins: **Waiting for you** (`attention: wait`, or your program told Herdr it
needs the person), **Stuck** (`block`), **Finished** (`done`), **Idle** (a silent panel),
**Working** (your program told Herdr it is working), **Idle** (anything else). Rows are projects.
Every card is your whole panel with its buttons. The person can answer you from the board without
opening your console, so make your `wait` line and your `actions` enough to decide on.
