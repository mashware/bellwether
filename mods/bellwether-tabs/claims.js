// What every console's producer claims (the `attention` of its panel.json), watched in one
// place for the whole window, and the interruptions it is worth: a notification and a sound
// when a console starts waiting for you or finishes. `block` interrupts nobody — it is stuck
// on something else, not on you (Ivy's D18). Consoles Herdr already counts as agents get Herdr's
// own interruptions instead: Claude Code reports itself, and for Codex the panel MCP reports the
// claim as Herdr's status (crates/bellwether-panel-mcp/src/status.rs).
import { panes, spaces, agents, run, focused, window, home, notify, playSound, exec, readTextFile, watchFolder } from "herdr";

export const RANK = { wait: 0, block: 1, done: 2 };

// Takes you to a console, wherever it is, with its status panel open.
export function go(terminal) {
  const pane = panes().find((p) => p.terminal === terminal);
  if (!pane) return;
  const space = spaces().find((s) => s.tabs.some((tb) => tb.id === pane.tab));
  if (space) run("space.select", { id: space.id });
  run("tab.select", { id: pane.tab });
  run("pane.focus", { terminal });
  run("layout.select", { stack: "panel-body", child: "bellwether" });
  run("layout.show", { id: "panel" });
}
const root = () => `${home()}/.local/state/bellwether`;

export class Claims {
  constructor() {
    this.now = {};      // terminal -> "wait" | "block" | "done"
    this.seeded = false; // the first read only learns: no toasts for what was already there
    this.pending = {};   // terminal -> when its toast went out, until you arrive or it expires
  }

  // Called on every render of the tab bar: arriving at a console whose toast is still out
  // means the toast was clicked (or you went there anyway), so its panel opens.
  arrived() {
    // Back to the window while a desktop notice is out (clicked, or not): go to its console.
    const active = Boolean(window()?.active);
    if (active && !this.wasActive && this.desktop && Date.now() - this.desktop.at < 600000) {
      const { terminal } = this.desktop;
      this.desktop = null;
      go(terminal);
    }
    this.wasActive = active;
    const t = focused()?.terminal;
    for (const [term, at] of Object.entries(this.pending)) if (Date.now() - at > 600000) delete this.pending[term];
    if (!t || !(t in this.pending)) return;
    delete this.pending[t];
    run("layout.select", { stack: "panel-body", child: "bellwether" });
    run("layout.show", { id: "panel" });
  }

  start(cx) {
    cx.spawn(async (cx) => {
      await exec("mkdir", ["-p", root()]).catch(() => null);
      for (;;) {
        await this.read(cx);
        try { await watchFolder(root()); } catch { await new Promise((r) => cx.timer.after(3000, r)); }
      }
    });
  }

  async read(cx) {
    const next = {};
    for (const p of panes()) {
      if (!p.terminal) continue;
      const text = await readTextFile(`${root()}/${p.terminal}/panel.json`).catch(() => null);
      if (!text) continue;
      let att;
      try { att = JSON.parse(text)?.attention; } catch { att = this.now[p.terminal]; } // half-written: keep it
      if (att in RANK) next[p.terminal] = att;
    }
    if (this.seeded) for (const [t, att] of Object.entries(next)) if (this.now[t] !== att) this.interrupt(t, att, cx);
    this.now = next; this.seeded = true;
    cx.notify();
  }

  // Claude Code and Codex report their own status to Herdr, so Herdr already interrupts for them
  // when they finish, and when they block on a question or a permission. Ours would say it twice.
  // But a producer can be waiting for you while its agent only says it finished its turn (Codex
  // asks in text and reports `done`), so a `wait` is given a moment for the agent's own `blocked`
  // to land, and only interrupts if Herdr is not already saying it.
  interrupt(terminal, att, cx) {
    if (att === "block") return;
    const agent = () => agents().find((a) => a.address === terminal);
    if (att === "done") {
      if (!agent()) this.fire(terminal, att);
      return;
    }
    cx.timer.after(4000, () => {
      if (this.now[terminal] !== "wait") return; // answered meanwhile
      if (agent()?.status === "blocked") return;   // Herdr is already saying it
      if (agent()?.status === "working") return;   // it carried on: nobody is waiting for you
      this.fire(terminal, att);
    });
  }

  fire(terminal, att) {
    // Herdr's own rule: no toast for the console you are already looking at.
    const active = Boolean(window()?.active);
    const looking = active && focused()?.terminal === terminal;
    const title = att === "wait" ? `${terminal} is waiting for you` : `${terminal} finished`;
    playSound(att === "wait" ? "blocked" : "done");
    if (looking) return;
    // A toast runs one app command, and a mod's own command does not reach it that way. So the
    // click focuses the console, and arriving there from this toast opens the status column.
    this.pending[terminal] = Date.now();
    notify({ title, level: att === "wait" ? "warning" : "success", action: { name: "pane.focus", args: { terminal } } });
    // Herdr's toast lives inside its window: with the window behind others, the desktop says it,
    // and its Open button brings Herdr to the front on that console.
    if (!active) {
      // Tied to herdr-app's desktop entry, a click makes GNOME raise Herdr itself (Wayland lets
      // no other program do it) and report the action, which takes you to the console. GNOME's
      // per-app settings for Herdr must allow banners and their content. Coming back to the
      // window any other way with the notice still out does the same (see arrived()).
      this.desktop = { terminal, at: Date.now() };
      exec("notify-send", ["-a", "Herdr", "-u", att === "wait" ? "critical" : "normal",
        "-h", "string:desktop-entry:herdr-app", "-A", "default=Open", "-A", "open=Open", "-w", title],
        { timeout: 600000 })
        .then((r) => {
          if (!["default", "open"].includes(r.stdout.trim())) return;
          this.desktop = null;
          go(terminal);
        })
        .catch(() => null);
    }
  }

  // The tab's claim is the strongest of its consoles': wait beats block beats done.
  tab(tabId) {
    let best = null;
    for (const p of panes()) {
      if (p.tab !== tabId || !p.terminal) continue;
      const att = this.now[p.terminal];
      if (att && (best === null || RANK[att] < RANK[best])) best = att;
    }
    return best;
  }
}
