// Bellwether's status panel on Herdr: paints the panel document (born as Ivy's, docs/04-panel-contract.md)
// that whoever works in a terminal publishes to
//   ~/.local/state/bellwether/<terminal>/panel.json
// It lives in the layout's right-hand column (layouts/bellwether.json, stack "panel-body", child "bellwether")
// and shows the focused console's panel. With props {terminal} it shows that console only.
import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { focused, panes, theme, settings, home, exec, hc, readTextFile, watchFolder } from "herdr";

const MARKS = { done: "✓", current: "▸", pending: "·", wait: "?", block: "⊘", info: "i" };
const ATTENTION = {
  wait: ["?", "it is waiting for you"],
  block: ["⊘", "stuck on something else, not you"],
  done: ["∎", "it finished and nobody looked"],
};
// Controls, zero-width and direction-changing scalars: a button carrying one vanishes (contract).
const REFUSED = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁠-⁤⁦-⁩﻿­]/;

function linkLabel(url) {
  try {
    const u = new URL(url);
    const seg = u.pathname.split("/").filter(Boolean);
    const last = seg[seg.length - 1] ?? "";
    if (seg.includes("merge_requests")) return `!${last} ↗`;
    if (seg.includes("pull") || seg.includes("issues")) return `#${last} ↗`;
    if (seg.includes("pipelines")) return `pipeline ${last} ↗`;
    return `${u.host}${last ? "/" + last : ""} ↗`;
  } catch { return "link ↗"; }
}

export default class BellwetherPanel extends View {
  init(p, cx) {
    this.fixed = p?.terminal ?? null; // with props.terminal it shows that console; else the focused one
    this.terminal = null; this.gen = 0;
    this.doc = null; this.error = null; this.mtime = null; this.sent = null;
    this.fold = {}; // terminal:block → folded; a click wins over the producer's `collapsed`
    this.follow(cx);
    // The stale notice depends on the clock, not on a write.
    cx.timer.every(30000, (cx) => cx.notify());
  }

  // Which console to show: the fixed one, else the focused terminal. While a mod or
  // nothing has the keyboard, it keeps the last console it showed.
  wanted() {
    return this.fixed ?? focused()?.terminal ?? this.terminal;
  }

  follow(cx) {
    const want = this.wanted();
    if (!want || want === this.terminal) return;
    this.terminal = want;
    this.dir = `${home()}/.local/state/bellwether/${want}`;
    this.file = `${this.dir}/panel.json`;
    this.doc = null; this.error = null; this.mtime = null; this.sent = null;
    const gen = ++this.gen, dir = this.dir;
    cx.spawn(async (cx) => {
      await exec("mkdir", ["-p", dir]).catch(() => null);
      while (gen === this.gen) {
        await this.load(cx, gen);
        try { await watchFolder(dir); }
        catch { await new Promise((r) => cx.timer.after(2000, r)); }
      }
    });
  }

  async load(cx, gen) {
    try {
      const text = await readTextFile(this.file);
      if (gen !== this.gen) return;
      if (text === null) { this.doc = null; this.error = null; this.mtime = null; cx.notify(); return; }
      const st = await exec("stat", ["-c", "%Y", this.file]).catch(() => null);
      if (st?.code === 0) this.mtime = Number(st.stdout.trim()) * 1000;
      const doc = JSON.parse(text);
      if (!doc || !Array.isArray(doc.lines)) throw new Error("it carries no lines");
      this.doc = doc; this.error = null;
    } catch (e) {
      this.error = `not the latest: ${e.message ?? e}`;
    }
    cx.notify();
  }

  send(text, cx) {
    this.sent = text;
    cx.spawn(async (cx) => {
      const r = await hc(["send", this.terminal, text]).catch((e) => ({ code: 1, stderr: String(e) }));
      this.sent = r.code === 0 ? null : `could not send: ${r.stderr.trim()}`;
      cx.notify();
    });
  }

  render(cx) {
    this.follow(cx);
    const t = theme();
    const mono = settings().font_family ?? "JetBrains Mono";
    const root = v_flex().id("bellwether-panel").size_full().min_h(0).rounded(10).overflow_hidden().border_1().border_color(t.border)
      .bg(t.background).text_color(t.foreground)
      .text_size(12).font_family(mono);
    if (!this.terminal) return root.p(12).child(div().text_color(t.dim).child("No terminal to follow"));

    const doc = this.doc;
    const pane = panes().find((p) => p.terminal === this.terminal);
    const rows = [];

    // Identity row: the console and the phase it says it is on.
    if (doc && doc.header !== false) {
      rows.push(h_flex().px(12).py(8).gap(8).items_center().border_b(1).border_color(t.border)
        .child(div().font_semibold().child(this.terminal))
        .child(div().text_color(t.dim).truncate().child(pane?.cwd?.split("/").pop() ?? ""))
        .when(Boolean(doc.phase), (el) => el.child(div().flex_1().text_right().text_color(t.accent).truncate().child(String(doc.phase)))));
    }

    // The application's own rows: claim, stale, unreadable. Not hidden by header:false.
    const stale = this.mtime && (Date.now() - this.mtime) > (Number(doc?.stale_after_minutes) || 30) * 60000;
    // A program Herdr sees working is not waiting for you, whatever its panel still claims.
    const claim = doc && !(doc.attention === "wait" && pane?.status === "working") && ATTENTION[doc.attention];
    if (claim) {
      const color = doc.attention === "wait" ? t.blocked : doc.attention === "block" ? t.ansi1 : t.done;
      const dimmed = stale && doc.attention !== "done";
      rows.push(h_flex().px(12).py(6).gap(8).items_center().bg(t.raised).text_color(dimmed ? t.dim : color)
        .child(div().font_bold().child(claim[0])).child(div().child(claim[1] + (dimmed ? " (nobody is holding it up)" : ""))));
    }
    if (stale) {
      const mins = Math.round((Date.now() - this.mtime) / 60000);
      rows.push(div().px(12).py(4).text_color(t.dim).child(`no news for ${mins} min`));
    }
    if (this.error) rows.push(div().px(12).py(4).text_color(t.ansi3).child(this.error));

    if (!doc) {
      return root.children(rows).child(v_flex().p(12).gap(6).text_color(t.dim)
        .child(div().child("Nothing published yet."))
        .child(div().text_size(11).child(this.file)));
    }

    // Body. A header's `ref` is not painted, so it does not widen the reference column.
    const isHeader = (l) => l && typeof l === "object" && l.style === "section";
    const refW = Math.min(12, Math.max(0, ...doc.lines.map((l) => (l && typeof l === "object" && !isHeader(l) && l.ref) ? String(l.ref).length : 0)));
    const STYLE = { dim: t.dim, title: t.foreground, accent: t.accent, ok: t.ansi2, warn: t.ansi3, error: t.ansi1 };
    const MARKC = { done: t.dim, current: t.accent, pending: t.muted, wait: t.blocked, block: t.ansi1, info: t.ansi4 };
    const body = v_flex().id("bellwether-body").flex_1().min_h(0).overflow_y_scrollbar().px(12).py(8).gap(2);
    // A block runs from its header to the next gap or header; a folded one shows only its header.
    let folded = false;
    doc.lines.forEach((l, i) => {
      if (l === "" || isHeader(l)) folded = false;
      if (folded) return;
      if (typeof l === "string") {
        body.child(l === "" ? div().h(8) : div().child(l.replace(/[\u0000-\u001f]/g, " ")));
        return;
      }
      if (!l || typeof l !== "object") return;
      const text = String(l.text ?? "").replace(/[\u0000-\u001f]/g, " ").slice(0, 512);
      // A block header: title, a rule, and the summary pinned right. When the column is narrow the
      // title is what gives way — the summary is the figure the reader came for, and the contract
      // already bounds it to MAX_SUMMARY (40) characters. Its mark, ref, link and indent are not
      // painted.
      if (isHeader(l)) {
        const summary = typeof l.summary === "string" ? [...l.summary.replace(/[\u0000-\u001f]/g, " ")].slice(0, 40).join("") : "";
        const id = `${this.terminal}:${l.key ?? i}`;
        const closed = this.fold[id] ?? l.collapsed === true;
        folded = closed;
        body.child(h_flex().id(`l${i}`).gap(8).items_center().when(i > 0, (e) => e.mt(6))
          .cursor_pointer().hover((s) => s.text_color(t.accent))
          .on_click((_e, cx) => { this.fold[id] = !closed; cx.notify(); })
          .child(div().flex_none().text_size(10).text_color(t.dim).child(closed ? "▸" : "▾"))
          .child(div().min_w(0).truncate().font_semibold().text_color(t.foreground).child(text))
          .child(div().flex_1().min_w(12).h(1).bg(t.border))
          .when(summary !== "", (el) => el.child(div().flex_none().text_color(t.muted).child(summary))));
        return;
      }
      const color = l.style ? STYLE[l.style] : l.mark === "done" ? t.dim : t.foreground;
      const link = typeof l.link === "string" && /^https?:\/\/[^\s/]+/.test(l.link) ? l.link : null;
      // A title with no mark and no ref names the whole panel: it starts at the left edge rather
      // than in the text column.
      const bare = l.style === "title" && !l.mark && !l.ref;
      // Indentation moves the text, not the row: the mark and ref columns stay aligned, and a
      // wrapped continuation lines up under the indented text.
      const indent = Math.min(64, Number(l.indent) || 0) * 7;
      body.child(h_flex().id(`l${i}`).gap(6).items_start()
        .when(!bare, (el) => el.child(div().w(12).flex_none().text_color(MARKC[l.mark] ?? t.dim).child(MARKS[l.mark] ?? "")))
        .when(!bare && refW > 0, (el) => el.child(div().w(refW * 7.5).flex_none().text_color(t.dim).truncate().child(l.ref ? String(l.ref) : "")))
        .child(div().flex_1().min_w(0).pl(indent).text_color(color ?? t.foreground).when(l.style === "title", (e) => e.font_semibold()).child(text))
        .when(Boolean(link), (el) => el.child(div().id(`k${i}`).flex_none().text_color(t.accent).cursor_pointer().tooltip(link)
          .hover((s) => s.text_color(t.foreground))
          .on_click(() => exec("xdg-open", [link])).child(linkLabel(link)))));
    });

    // Buttons: the command is painted under its label, and a click sends exactly that.
    const actions = (Array.isArray(doc.actions) ? doc.actions : [])
      .filter((a) => a && typeof a.label === "string" && typeof a.send === "string"
        && a.send.length <= 512 && !REFUSED.test(a.send) && !REFUSED.test(a.label))
      .slice(0, 8);
    const bar = v_flex().px(12).py(8).gap(6).border_t(1).border_color(t.border);
    actions.forEach((a, i) => bar.child(v_flex().id(`a${i}`).px(10).py(6).rounded(6).border_1().border_color(t.input)
      .cursor_pointer().hover((s) => s.bg(t.hover)).on_click((_e, cx) => this.send(a.send, cx))
      .child(div().font_semibold().child(a.label))
      .child(div().text_size(11).text_color(t.dim).truncate().child(`$ ${a.send}`))));
    if (this.sent && this.sent.startsWith("could not")) bar.child(div().text_color(t.ansi1).child(this.sent));

    return root.children(rows).child(body).when(actions.length > 0, (el) => el.child(bar));
  }
}
