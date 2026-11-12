# FLOW.md

Config for the `flow` plugin in this repo; the `/flow:*` commands read it in their step 0.
Absent key = default / auto-discover. Every key and default: the plugin's `examples/FLOW.template.md`.

Bellwether is the project's name. The repository has no remote yet: the `tracker` and `git` keys below
assume it will live on GitHub under `mashware`, like the user's other personal projects. Until then
a work starts without a ticket.

## tracker
- tool: gh
- view_cmd: gh issue view {TICKET}
- comments_cmd: gh issue view {TICKET} --comments

## git

`diff_exclude` keeps `Cargo.lock` out of the diff every phase reads and every reviewer is
launched over.

- host: github
- cli: gh
- default_base: origin/main
- diff_exclude:
  - 'Cargo.lock'
- branch_pattern: {TICKET}-{slug}
- assignee: mashware
- squash: true
- request_sections:
  - What it does
  - Why
  - How to test it
  - What is not included
- worktree: always
- worktree_path: .worktrees/{branch}

## autonomy
- mode: auto

## quality

Two halves, one gate each: the Rust workspace (the panel MCP and the panel contract it carries) and
the Herdr mods (JavaScript, checked by Herdr itself). `scripts/check-mods.sh` checks the mods straight
from this folder; `scripts/sync.sh` is the one that installs them into the user's Herdr, so it is a
functional step, never a static one.

`sensitive_paths` names what is risky here. The MCP decides which console a process publishes for
(from `HC_TERMINAL_ID`) and writes into the user's home. `bellwether-panel` turns a panel's `actions`
into text typed into a console (`hc send`) and opens links. `claims.js` runs desktop notifications
and moves the user between consoles. `panel-contract` is the document every producer and the mods
agree on.

- test: cargo test --workspace
- test_one: cargo test --workspace {FILTER}
- static_analysis: cargo clippy --workspace --all-targets -- -D warnings && scripts/check-mods.sh
- style_fix: cargo fmt --all
- functional_check: Run `scripts/sync.sh` and `herdr-app do layout.switch --name bellwether` (the user's own layout stays; `ctrl-shift-l` switches back). Drive the window with `herdr-app do …` (`herdr-app commands` lists them) and publish test panels as files under `~/.local/state/bellwether/<terminal>/panel.json` for a console you created for the check, or through `target/release/bellwether-panel-mcp` with `HC_TERMINAL_ID` set. Evidence is `herdr-app shot`, but it returns the last painted frame: with the window behind others it is stale, so ask the user to bring Herdr to the front before trusting a picture. Desktop notifications cannot be tested from a sandboxed shell (it reports "Actions are not supported", which is false): ask the user to run them in a Herdr console. Clicks, sounds and anything else only a person can do stay needs-manual, with the exact steps.
- sensitive_paths:
  - 'crates/bellwether-panel-mcp/**'
  - 'crates/panel-contract/**'
  - 'mods/bellwether-panel/**'
  - 'mods/bellwether-tabs/claims.js'

The following reviewers are Claude Code plugin agents. Each harness without them must mask this
list in its own overlay.

- reviewers:
  - pr-review-toolkit:silent-failure-hunter
  - pr-review-toolkit:comment-analyzer
  - pr-review-toolkit:pr-test-analyzer
  - pr-review-toolkit:type-design-analyzer

## agents
- fanout_max: 4
- budget_max: 12

## conventions

Not style: each one comes from a concrete mistake or a real risk, so review treats them as blocking.

- **Bellwether never infers state.** No parsing a console's painting, an agent's private files or
  Herdr's screen heuristics to guess how a task is going: the mods paint only what a producer
  publishes, and `attention` is the one field they act on.
- **Nothing is typed into a console on Bellwether's own initiative.** The only input that reaches a
  console's program is a panel button a person pressed. The one output Bellwether writes is Herdr's
  status sequence (OSC 7501), from the panel MCP, in one short write, and only on a console whose
  program does not report its own status — Claude Code does, and its word is never overwritten.
- A panel's `actions` are **code execution** in the user's shell: the command is painted before it
  runs, and the text sent is plain — no interpolation, no templates. A button carrying a control,
  zero-width or direction-changing character vanishes rather than being cleaned.
- A `panel.json` from a foreign origin (a cloned repo, a container) is **untrusted**: never run
  anything from it without visible confirmation.
- **Herdr's shipped mods and layouts are never edited**: clone them (`mod.clone`, `layout.clone`)
  and change the clone; Herdr replaces its own files on every start.
- **Mods animate natively, never with a timer**, and poll only data, gently: every redraw of a mod
  redraws the whole window.
- **Tests get mutated.** A test that does not fail when you deliberately break what it claims to
  cover covers nothing. The usual disguises: asserting on where a value is recorded instead of on
  its effect, and picking numbers where the defect and the fix give the same result.
- **Everything written goes in English** — code, comments, docs, commits, branches and the PR body.
  Spanish stays in the working chat only.

## notes

- all: Read `README.md` first. Herdr's mod API is documented in the running app's
  `~/.config/herdr-app/AGENTS.md`, `herdr.d.ts` and `gpui-kit.d.ts`; read them before writing a
  mod, they are rewritten for each Herdr version. `crates/panel-contract` started as a copy of
  Ivy's; it is ours now, and the panel document's rules live there.
- feat:review: Check the `conventions` rules explicitly against the diff: anything written into a
  console, any state inferred from someone else's output, and the origin of every `panel.json`
  read. Then mutate the new tests — break what each one claims to cover and confirm it fails.
- bug:validate: Same mutation rule: the regression test must fail with the fix reverted.
- work:watch: Not applicable — desktop mods and a local MCP, there is no server deploy to monitor.

## knowledge

There is no domain-memory store in this repository yet, so knowledge is searched in its own files.

- search:
  - rg -n -i "{QUERY}" README.md mods crates
