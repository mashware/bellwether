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

## Install and develop

```sh
scripts/sync.sh
herdr-app do layout.switch --name bellwether
```

Edit here, then run `scripts/sync.sh` again: it copies the mods and the layout into
`~/.config/herdr-app/` and runs `herdr-app check` on each mod. `ctrl-shift-l` switches back to
Herdr's own layout.

## Licence

GPL-3.0-only. See [`LICENSE`](LICENSE).
