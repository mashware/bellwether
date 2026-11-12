# Bellwether

A status panel and a board for coding agents, as mods for the [Herdr](https://herdr.dev) desktop app.

Whoever works in a console — an agent, a CI run, a script — publishes how it is going in
`~/.local/state/bellwether/<terminal>/panel.json`, or through the `bellwether` MCP. The document is
specified in [`docs/panel-contract.md`](docs/panel-contract.md). Herdr only paints it.

## For AI agents

If you are an agent working in a Herdr console, read [`docs/for-agents.md`](docs/for-agents.md). It
says how to publish your status panel, when to claim the person's attention, how to publish your
project's roadmap, and what the board does with all of it.

## What is here

| Folder | What it does |
|---|---|
| `mods/bellwether-panel` | The status panel: the focused console's `panel.json`, in the window's right-hand column |
| `mods/bellwether-pane` | Each console's header: the claim its producer declares (`?` `⊘` `∎`) and a button for the panel |
| `mods/bellwether-tabs` | Herdr's tab bar plus **Status** and **Board** buttons; hides the column while the board shows |
| `mods/bellwether-board` | A kanban of every console in every space: state columns (waiting for you, stuck, working, finished, idle), one row per git repository with its roadmap folded, and each card is the console's whole panel with its buttons |
| `mods/bellwether-menu` | A terminal's right-click menu: a clone of Herdr's `menu` with **Copy** and **Paste** on top, set as the layout's `terminal_menu` |
| `themes/bellwether.json` | The layout's terminal colors: the palette of the user's Tilix profile (base16 Twilight dark) |
| `layouts/bellwether.json` | Herdr's main layout with the five mods wired in |

## The board's columns

A card's column follows one fixed rule, first match wins, and nothing in it is guessed:

1. **Waiting for you**: the panel says `attention: wait`, or the program itself reported to Herdr
   that it needs you (`blocked`) even without a panel.
2. **Stuck**: `attention: block`.
3. **Finished**: `attention: done`, or Herdr's `done`.
4. **Idle**: a panel silent for longer than its `stale_after_minutes`. A wait never goes stale: a
   console that waits publishes nothing.
5. **Working**: Herdr's `working`.
6. **Idle**: everything else.

Waiting for you is ordered by `attention_since`, oldest first, and a card warms up after 5 minutes
and more after 15.

## Install

With the [Herdr](https://herdr.dev) desktop app installed, on Linux or macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/mashware/bellwether/main/install.sh | sh
```

That is all. The script downloads the MCP server for your platform from the latest release into
`~/.local/bin` and the mods, the layout and its theme into `~/.config/herdr-app/`; registers the
server with Claude Code and Codex when they are installed; and switches Herdr to the layout
(`ctrl-shift-l` switches back). Restart your agent sessions afterwards so they load the server. Run
it again to update. Nothing is cloned and Rust is not needed.

The layout brings its own terminal colors (`themes/bellwether.json`, base16 Twilight): remove the
`"theme"` line from `~/.config/herdr-app/layouts/bellwether.json` to keep yours.

<details>
<summary>From source</summary>

With `git` and a Rust toolchain ([rustup](https://rustup.rs) picks the version this repository
pins):

```sh
git clone https://github.com/mashware/bellwether
cd bellwether
scripts/sync.sh
herdr-app do layout.switch --name bellwether
cargo build --release -p bellwether-panel-mcp
install -m 755 target/release/bellwether-panel-mcp ~/.local/bin/
claude mcp add -s user bellwether -- ~/.local/bin/bellwether-panel-mcp
```

For Codex, add to `~/.codex/config.toml` (Codex passes only the variables it is told to, and the
server needs Herdr's `HC_TERMINAL_ID`):

```toml
[mcp_servers.bellwether]
command = "/home/<you>/.local/bin/bellwether-panel-mcp"
env_vars = ["HC_TERMINAL_ID", "BELLWETHER_STATE"]
```

To update: `git pull`, run `scripts/sync.sh` again, rebuild and reinstall the server, and restart
the agent sessions.

</details>

## Works with flow

[flow](https://github.com/mashware/flow-workflows) — guided feature and bug workflows for Claude
Code, Codex, opencode and Gemini CLI — publishes its live panel to Bellwether with no setup: once
the `bellwether` MCP is registered, every flow work shows its phase, its MRs, the decision it is
waiting on and the buttons to answer it, on its console and on the board.

## Develop

Edit here, then run `scripts/sync.sh` again: it copies the mods, the layout and the theme into
`~/.config/herdr-app/` and runs `herdr-app check` on each mod. `scripts/check-mods.sh` checks the
mods without installing them; `cargo test --workspace` runs the Rust side. A release is a version
tag: `git tag v0.1.0 && git push origin v0.1.0` builds the server for Linux and macOS and attaches
it to the GitHub release that `install.sh` downloads.

## Licence

GPL-3.0-only. See [`LICENSE`](LICENSE).
