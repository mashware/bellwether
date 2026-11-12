// Bellwether's pane header for Herdr: the program, the project (the git repository's folder, so a
// worktree reads as its project) and the title of the task its producer publishes — e.g.
// "Claude Code  tax-platform: MT-16576 · Tax rate changes" —, the claim it declares (? ⊘ ∎) and a button that shows this console's status panel in the layout's
// right-hand column (the bellwether-panel mod), or hides it.
import { View, div, svg, image } from "gpui-kit";
import { h_flex } from "gpui-base";
import { panes, props, run, theme, home, layout, readTextFile, watchFolder, exec } from "herdr";

const ATT = { wait: ["?", "blocked"], block: ["⊘", "ansi1"], done: ["∎", "done"] };

function paneFrame(node) {
  if (!node || node.hidden) return null;
  if (node.content?.tab && node.content.pane) return node.content.pane;
  let children = node.children ?? [];
  if (node.type === "stack") {
    const visible = children.filter(child => !child.hidden);
    children = [visible.find(child => child.id === node.active) ?? visible[0]];
  }
  for (const child of children) {
    const frame = paneFrame(child);
    if (frame) return frame;
  }
  return null;
}

function icon(p, size, color) {
  if (!p?.icon) return null;
  return p.tint ? svg(p.icon).size(size).flex_none().text_color(color) : image(p.icon).w(size).h(size).flex_none();
}

// The last folder of a path, or ~ for the home folder.
function lastFolder(path) {
  if (!path) return "";
  return path === home() ? "~" : path.split("/").filter(Boolean).pop() ?? path;
}

// The main checkout's folder for a working directory, asked of git; null outside a repository.
const roots = {};
async function rootOf(cwd) {
  if (cwd in roots) return roots[cwd];
  const r = await exec("git", ["-C", cwd, "rev-parse", "--path-format=absolute", "--git-common-dir"]).catch(() => null);
  const common = r?.code === 0 ? r.stdout.trim() : null;
  roots[cwd] = common ? (common.endsWith("/.git") ? common.slice(0, -5) : common) : null;
  return roots[cwd];
}

function findNode(node, id) {
  if (!node) return null;
  if (node.id === id) return node;
  for (const child of node.children ?? []) { const f = findNode(child, id); if (f) return f; }
  return null;
}

// Whether the right-hand column is showing the status panel.
function columnOpen() {
  const root = layout(), panel = findNode(root, "panel"), body = findNode(root, "panel-body");
  return Boolean(panel && !panel.hidden && body?.active === "bellwether");
}

export default class Pane extends View {
  init(p, cx) {
    this.hovered = false; this.attention = null; this.published = false; this.title = null; this.cwd = null; this.project = null;
    const terminal = p?.terminal ?? props()?.terminal ?? panes().find((x) => x.pane === props()?.pane)?.terminal;
    if (!terminal) return;
    const dir = `${home()}/.local/state/bellwether/${terminal}`;
    cx.spawn(async (cx) => {
      await exec("mkdir", ["-p", dir]).catch(() => null);
      for (;;) {
        await this.read(dir, terminal, cx);
        try { await watchFolder(dir); } catch { await new Promise((r) => cx.timer.after(3000, r)); }
      }
    });
  }

  async read(dir, terminal, cx) {
    const text = await readTextFile(`${dir}/panel.json`).catch(() => null);
    let doc = null;
    try { doc = text ? JSON.parse(text) : null; } catch { return; } // half-written: keep the last claim
    this.published = Boolean(doc);
    this.attention = doc?.attention ?? null;
    const title = Array.isArray(doc?.lines) ? doc.lines.find((l) => l && typeof l === "object" && l.style === "title") : null;
    this.title = title ? String(title.text ?? "").replace(/[\u0000-\u001f]/g, " ").slice(0, 200) : null;
    cx.notify();
  }

  // The column follows the focused console: focusing this one is what points it here.
  toggle(focusedHere) {
    if (columnOpen() && focusedHere) { run("layout.hide", { id: "panel" }); return; }
    run("pane.focus", { pane: props().pane });
    run("layout.select", { stack: "panel-body", child: "bellwether" });
    run("layout.show", { id: "panel" });
  }

  me() {
    return panes().find((p) => p.pane === props()?.pane);
  }

  button(t, id, icon, command, args) {
    return div().id(id).p(3).rounded(4).text_color(t.dim)
      .hover((s) => s.bg(t.hover).text_color(t.foreground))
      .on_click((_e, cx) => { cx.stop_propagation(); run(command, args); })
      .child(svg(`mods/bellwether-pane/${icon}.svg`).size(14));
  }

  render(cx) {
    const t = theme();
    const p = this.me();
    if (!p) return div();
    const name = ((p.program?.agent || p.program?.shell) && p.program.name) || p.title || p.program?.name || "shell";
    const pic = icon(p.program, 14, t.dim);
    const st = p.status;
    const pane = p.pane;
    const bottom = paneFrame(layout())?.header?.position === "bottom";
    const terminal = p.terminal;
    // The project's name follows the console's live folder; git is asked once per folder.
    if (p.cwd !== this.cwd) {
      this.cwd = p.cwd; this.project = lastFolder(p.cwd);
      if (p.cwd) cx.spawn(async (cx) => { const root = await rootOf(p.cwd); if (root && this.cwd === p.cwd) { this.project = lastFolder(root); cx.notify(); } });
    }
    const open = Boolean(terminal) && p.focused && columnOpen();
    const claim = ATT[this.attention];
    return h_flex().id(`pane-head-${pane}`).size_full().px(10).gap(8).items_center().bg(t.background)
      .when(bottom, el => el.border_t(1)).when(!bottom, el => el.border_b(1)).border_color(t.border)
      .text_size(12)
      .on_hover((hovered, cx) => { this.hovered = hovered; cx.notify(); })
      .on_click(() => run("pane.focus", { pane }))
      .when(Boolean(pic), (el) => el.child(pic))
      .child(div().flex_none().truncate().max_w(260).font_medium().text_color(t.foreground).child(name))
      .child(h_flex().flex_1().min_w(0).gap(0)
        .child(div().flex_none().text_color(t.dim).child(this.title ? `${this.project}:` : this.project ?? ""))
        .when(Boolean(this.title), (el) => el.child(div().flex_1().min_w(0).pl(6).truncate().text_color(t.foreground).child(this.title))))
      .when(st === "working", el => el.child(svg("mods/workspace-chrome/spinner.svg").size(12).flex_none().text_color(t.working).spin(900)))
      .when(st === "blocked", el => el.child(div().flex_none().px(6).h(18).flex().items_center().rounded(5).bg(t.blocked + "26").text_size(10.5).font_semibold().text_color(t.blocked).child("Needs you")))
      .when(st === "done", el => el.child(div().size(7).flex_none().rounded(4).bg(t.done)))
      // The claim the producer declares, visible even with the panel hidden.
      .when(Boolean(claim), el => el.child(div().flex_none().font_bold().text_color(t[claim[1]]).child(claim[0])))
      .when(this.hovered, el => el
        .child(this.button(t, `ph-r-${pane}`, "split-right", "pane.split", { direction: "right", pane }))
        .child(this.button(t, `ph-d-${pane}`, "split-down", "pane.split", { direction: "down", pane }))
        .child(this.button(t, `ph-z-${pane}`, p.zoomed ? "unzoom" : "zoom", "pane.zoom", { pane }))
        .child(this.button(t, `ph-x-${pane}`, "close", "pane.close", { pane })))
      // Always there on a console: show or hide its status panel.
      .when(Boolean(terminal), el => el.child(div().id(`ph-bellwether-${pane}`).p(3).rounded(4)
        .text_color(open ? t.accent : this.published ? t.muted : t.dim)
        .when(open, (e) => e.bg(t.selected))
        .hover((s) => s.bg(t.hover).text_color(t.foreground))
        .tooltip(open ? "Hide status panel" : "Show status panel")
        .on_click((_e, cx) => { cx.stop_propagation(); this.toggle(p.focused); cx.notify(); })
        .child(svg("mods/workspace-chrome/panel-right.svg").size(14))));
  }
}
