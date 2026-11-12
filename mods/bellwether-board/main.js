// Bellwether's board on Herdr: a kanban of every console in every space. Columns are states, rows
// are projects (the git repository a console works in, worktrees included), and every card is that
// console's whole status panel, read from ~/.local/state/bellwether/<terminal>/panel.json.
// Each project row carries its roadmap, folded, read from ~/.local/state/bellwether/projects/<key>/plan.json.
//
// Nothing here infers state. A card's column comes from what its producer declared (`attention`)
// or what its program reported to Herdr (`status`), by one fixed rule; a roadmap's progress is a
// count of what the plan marks done. The only thing typed into a console is a button somebody pressed.
import { View, div } from "gpui-kit";
import { v_flex, h_flex } from "gpui-base";
import { panes, spaces, theme, settings, home, run, exec, hc, readTextFile, watchFolder } from "herdr";

const ROOT = () => `${home()}/.local/state/bellwether`;
const MARKS = { done: "✓", current: "▸", pending: "·", wait: "?", block: "⊘", info: "i" };
// Controls, zero-width and direction-changing scalars: a button carrying one vanishes (contract).
const REFUSED = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁠-⁤⁦-⁩﻿­]/;
const COLUMNS = [
  { id: "wait", sym: "?", name: "Waiting for you", grow: 1.3 },
  { id: "block", sym: "⊘", name: "Stuck", grow: 1.15 },
  { id: "working", sym: "▸", name: "Working", grow: 1.15 },
  { id: "done", sym: "∎", name: "Finished", grow: 0.75 },
  { id: "idle", sym: "·", name: "Idle", grow: 0.7 },
];
const RANK = { wait: 0, block: 1, working: 2, done: 3, idle: 4 };
const CAP = 18; // lines a card shows before "Show all"
const WARM = 5 * 60000, HOT = 15 * 60000; // how long a console has waited before it warms up

const isObj = (l) => l && typeof l === "object";
const isHeader = (l) => isObj(l) && l.style === "section";
const clean = (s) => String(s ?? "").replace(/[\u0000-\u001f]/g, " ");

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

// The name of a project's state folder, from its repository root. Must match the Rust
// `panel_contract::project_key`: the whole path, with % and / escaped.
const projectKey = (root) => root.replaceAll("%", "%25").replaceAll("/", "%2F");

// The column a card goes in. A wait never goes stale: a console that waits publishes nothing.
function columnOf(c) {
  const a = c.doc?.attention;
  if (a === "wait") return "wait";
  if (a === "block") return "block";
  if (a === "done" || c.status === "done") return "done";
  if (c.status === "blocked") return "wait"; // the program itself reported it needs you
  if (c.stale) return "idle";
  if (c.status === "working") return "working";
  return "idle";
}

const titleOf = (c) => {
  const l = c.doc?.lines?.find((l) => isObj(l) && l.style === "title");
  return l ? clean(l.text) : (c.program || c.terminal);
};

export default class Board extends View {
  init(_p, cx) {
    this.docs = {}; this.mtimes = {}; this.roots = {}; this.plans = {};
    this.rmOpen = {};   // project → roadmap shown
    this.fold = {};     // terminal:block → folded (a click wins over the producer's default)
    this.full = {};     // terminal → every line shown
    this.opened = {};   // terminal → a finished/idle card shown whole
    this.msFull = {};   // project:milestone → every deliverable shown
    this.sent = {};     // terminal → why a button could not send
    cx.spawn(async (cx) => {
      await exec("mkdir", ["-p", ROOT()]).catch(() => null);
      for (;;) {
        await this.reload(cx);
        try { await watchFolder(ROOT()); } catch { await new Promise((r) => cx.timer.after(3000, r)); }
      }
    });
    // Ages and stale marks follow the clock; a new console's repository is looked up then too.
    cx.timer.every(30000, (cx) => { cx.spawn(async (cx) => this.reload(cx)); });
  }

  async reload(cx) {
    const docs = {}, mtimes = {};
    for (const p of panes()) {
      if (!p.terminal) continue;
      const file = `${ROOT()}/${p.terminal}/panel.json`;
      const text = await readTextFile(file).catch(() => null);
      if (text) {
        try { const d = JSON.parse(text); if (Array.isArray(d.lines)) docs[p.terminal] = d; } catch { docs[p.terminal] = this.docs[p.terminal]; }
        const st = await exec("stat", ["-c", "%Y", file]).catch(() => null);
        if (st?.code === 0) mtimes[p.terminal] = Number(st.stdout.trim()) * 1000;
      }
      if (p.cwd && !(p.cwd in this.roots)) this.roots[p.cwd] = await this.rootOf(p.cwd);
    }
    const plans = {};
    for (const root of new Set(Object.values(this.roots).filter(Boolean))) {
      const text = await readTextFile(`${ROOT()}/projects/${projectKey(root)}/plan.json`).catch(() => null);
      if (!text) continue;
      try { const plan = JSON.parse(text); if (Array.isArray(plan.milestones)) plans[root] = plan; } catch { plans[root] = this.plans[root]; }
    }
    this.docs = docs; this.mtimes = mtimes; this.plans = plans;
    cx.notify();
  }

  // The main checkout's folder for a working directory, asked of git; null outside a repository.
  async rootOf(cwd) {
    const r = await exec("git", ["-C", cwd, "rev-parse", "--path-format=absolute", "--git-common-dir"]).catch(() => null);
    if (r?.code !== 0) return null;
    const common = r.stdout.trim();
    return common.endsWith("/.git") ? common.slice(0, -5) : common;
  }

  go(c) {
    run("space.select", { id: c.space });
    run("tab.select", { id: c.tab });
    run("pane.focus", { terminal: c.terminal });
    run("layout.select", { stack: "panel-body", child: "bellwether" });
    run("layout.show", { id: "panel" });
  }

  send(c, text, cx) {
    delete this.sent[c.terminal];
    cx.spawn(async (cx) => {
      const r = await hc(["send", c.terminal, text]).catch((e) => ({ code: 1, stderr: String(e) }));
      if (r.code !== 0) this.sent[c.terminal] = `could not send: ${String(r.stderr ?? "").trim()}`;
      cx.notify();
    });
  }

  // ─── The card: the console's panel ───────────────────────────────────────────────────────────

  body(c, t) {
    const lines = c.doc.lines;
    const refW = Math.min(12, Math.max(0, ...lines.map((l) => (isObj(l) && !isHeader(l) && l.ref) ? String(l.ref).length : 0)));
    const STYLE = { dim: t.dim, title: t.foreground, accent: t.accent, ok: t.ansi2, warn: t.ansi3, error: t.ansi1 };
    const MARKC = { done: t.dim, current: t.accent, pending: t.muted, wait: t.blocked, block: t.ansi1, info: t.ansi4 };
    const rows = [];
    let folded = false, skippedTitle = false, shown = 0;
    const total = lines.length;
    const capped = !this.full[c.terminal] && total > CAP;
    lines.forEach((l, i) => {
      // The title is the card's own first row.
      if (!skippedTitle && isObj(l) && l.style === "title") { skippedTitle = true; return; }
      if (rows.length === 0 && l === "") return;
      // A block runs from its header to the next gap or header.
      if (l === "" || isHeader(l)) folded = false;
      if (folded) return;
      if (capped && shown >= CAP) return;
      shown++;
      if (typeof l === "string") { rows.push(l === "" ? div().h(6) : div().child(clean(l))); return; }
      if (!isObj(l)) return;
      const text = clean(l.text).slice(0, 512);
      if (isHeader(l)) {
        const id = `${c.terminal}:${l.key ?? i}`;
        const closed = this.fold[id] ?? l.collapsed === true;
        folded = closed;
        const summary = typeof l.summary === "string" ? [...clean(l.summary)].slice(0, 40).join("") : "";
        rows.push(h_flex().id(`h-${id}`).gap(6).items_center().cursor_pointer().when(rows.length > 0, (e) => e.mt(4))
          .hover((s) => s.text_color(t.accent))
          .on_click((_e, cx) => { cx.stop_propagation(); this.fold[id] = !closed; cx.notify(); })
          .child(div().flex_none().text_size(10).text_color(t.dim).child(closed ? "▸" : "▾"))
          .child(div().min_w(0).truncate().font_semibold().child(text))
          .child(div().flex_1().min_w(10).h(1).bg(t.border))
          .when(summary !== "", (el) => el.child(div().flex_none().text_color(t.muted).child(summary))));
        return;
      }
      const color = l.style ? STYLE[l.style] : l.mark === "done" ? t.dim : t.foreground;
      const link = typeof l.link === "string" && /^https?:\/\/[^\s/]+/.test(l.link) ? l.link : null;
      const indent = Math.min(64, Number(l.indent) || 0) * 7;
      rows.push(h_flex().id(`l-${c.terminal}-${i}`).gap(6).items_start()
        .child(div().w(12).flex_none().text_color(MARKC[l.mark] ?? t.dim).child(MARKS[l.mark] ?? ""))
        .when(refW > 0, (el) => el.child(div().w(refW * 7.5).flex_none().text_color(t.dim).truncate().child(l.ref ? clean(l.ref) : "")))
        .child(div().flex_1().min_w(0).pl(indent).text_color(color ?? t.foreground)
          .when(l.mark === "wait", (e) => e.font_semibold()).child(text))
        .when(Boolean(link), (el) => el.child(div().id(`k-${c.terminal}-${i}`).flex_none().text_color(t.accent).cursor_pointer().tooltip(link)
          .hover((s) => s.text_color(t.foreground)).on_click((_e, cx) => { cx.stop_propagation(); exec("xdg-open", [link]); }).child(linkLabel(link)))));
    });
    const box = v_flex().px(10).pb(8).gap(2).children(rows);
    if (capped) box.child(div().id(`more-${c.terminal}`).mt(4).text_size(11).text_color(t.accent).cursor_pointer()
      .on_click((_e, cx) => { cx.stop_propagation(); this.full[c.terminal] = true; cx.notify(); }).child(`Show all ${total} lines`));
    return box;
  }

  actions(c, t) {
    const actions = (Array.isArray(c.doc.actions) ? c.doc.actions : [])
      .filter((a) => a && typeof a.label === "string" && typeof a.send === "string"
        && a.send.length <= 512 && !REFUSED.test(a.send) && !REFUSED.test(a.label))
      .slice(0, 8);
    if (!actions.length) return null;
    const bar = h_flex().flex_wrap().px(10).py(8).gap(6).border_t(1).border_color(t.border);
    actions.forEach((a, i) => bar.child(v_flex().id(`a-${c.terminal}-${i}`).max_w(260).px(8).py(4).rounded(6).border_1().border_color(t.input)
      .cursor_pointer().hover((s) => s.bg(t.hover).border_color(t.accent)).on_click((_e, cx) => { cx.stop_propagation(); this.send(c, a.send, cx); })
      .child(div().font_semibold().child(clean(a.label)))
      .child(div().text_size(10).text_color(t.dim).truncate().child(`$ ${a.send}`))));
    if (this.sent[c.terminal]) bar.child(div().w_full().text_color(t.ansi1).child(this.sent[c.terminal]));
    return bar;
  }

  card(c, t) {
    const col = c.column;
    const COLOR = { wait: t.blocked, block: t.ansi1, working: t.working, done: t.done, idle: t.border };
    const waited = col === "wait" && c.since ? Date.now() - c.since : 0;
    const heat = waited >= HOT ? "hot" : waited >= WARM ? "warm" : null;
    const compact = (col === "done" || col === "idle") && !this.opened[c.terminal];
    const d = c.doc;
    const card = v_flex().id(`card-${c.terminal}`).w_full().min_w(0).rounded(8).bg(t.surface)
      .when(heat === "warm", (e) => e.bg(t.blocked + "14"))
      .when(heat === "hot", (e) => e.bg(t.blocked + "29"))
      .border_color(COLOR[col]).when(heat === "hot", (e) => e.border_2()).when(heat !== "hot", (e) => e.border_1())
      .when(Boolean(c.stale), (e) => e.opacity(0.6))
      // A click anywhere on the card goes to its console; links, buttons and folds inside stop it.
      .cursor_pointer().hover((s) => s.border_color(t.accent)).on_click(() => this.go(c));
    card.child(h_flex().gap(8).items_center().px(10).pt(8).pb(compact ? 8 : 4)
      .child(div().flex_1().min_w(0).truncate().font_semibold().text_size(13).text_color(d ? t.foreground : t.dim).child(titleOf(c)))
      .when(Boolean(d?.phase), (el) => el.child(div().flex_none().text_color(t.accent).child(clean(d.phase))))
      .child(div().id(`go-${c.terminal}`).flex_none().px(2).text_color(t.dim).tooltip("Go to this console")
        .hover((s) => s.text_color(t.accent)).child("⤢")));
    if (d && !compact) {
      card.child(this.body(c, t));
      const bar = this.actions(c, t);
      if (bar) card.child(bar);
    }
    const foot = [];
    if (c.branch) foot.push(div().flex_1().min_w(0).truncate().child(`⎇ ${c.branch}`));
    if (!d) foot.push(div().child("publishes nothing"));
    if (c.stale) foot.push(div().flex_none().child(`no news for ${Math.round((Date.now() - c.mtime) / 60000)} min`));
    if (d && (col === "done" || col === "idle")) foot.push(div().id(`open-${c.terminal}`).flex_none().text_color(t.accent).cursor_pointer()
      .on_click((_e, cx) => { cx.stop_propagation(); this.opened[c.terminal] = !this.opened[c.terminal]; cx.notify(); })
      .child(compact ? "Show panel" : "Hide panel"));
    if (foot.length) card.child(h_flex().gap(8).items_center().px(10).py(6).border_t(1).border_color(t.border)
      .text_size(11).text_color(t.dim).children(foot));
    return card;
  }

  // ─── The project's roadmap ───────────────────────────────────────────────────────────────────

  roadmap(p, t) {
    const plan = p.plan;
    if (!plan) {
      const heads = [];
      for (const c of p.cards) for (const l of c.doc?.lines ?? []) if (isHeader(l)) heads.push([titleOf(c), l]);
      if (!heads.length) return null;
      return {
        mini: div().text_color(t.dim).child(`no plan · ${heads.length} blocks`),
        body: () => v_flex().gap(6)
          .child(h_flex().flex_wrap().gap(6).children(heads.map(([title, l], i) => h_flex().id(`rb-${p.root}-${i}`).gap(6).items_center()
            .px(8).py(4).rounded(6).border_1().border_color(t.border).bg(t.surface)
            .child(div().text_color(t.dim).child(`${title} ·`)).child(div().font_semibold().child(clean(l.text)))
            .child(div().text_color(t.muted).child(clean(l.summary ?? ""))))))
          .child(div().text_size(11).text_color(t.dim).child("No plan published: these are the block headers of this project's consoles, in no particular order.")),
      };
    }
    const ms = plan.milestones.filter((m) => m && typeof m.id === "string" && m.id);
    const items = (m) => (Array.isArray(m.items) ? m.items : []).map((it) => typeof it === "string" ? { text: it } : (isObj(it) ? it : null)).filter(Boolean);
    const progress = (m) => { const its = items(m); return [its.filter((i) => i.mark === "done").length, its.length]; };
    const here = (m) => p.cards.filter((c) => c.doc?.milestone === m.id);
    let D = 0, T = 0, msDone = 0;
    for (const m of ms) { const [d, n] = progress(m); D += d; T += n; if (n > 0 && d === n) msDone++; }
    const head = [];
    for (const m of ms) { const [d, n] = progress(m); if (n > 0 && d === n && !here(m).length) head.push(m); else break; }
    const rest = ms.slice(head.length);
    const started = rest.filter((m) => {
      const [d, n] = progress(m);
      if (n > 0 && d === n) return here(m).length > 0;
      return d > 0 || here(m).length > 0 || items(m).some((i) => ["current", "wait", "block"].includes(i.mark));
    });
    const later = rest.filter((m) => !started.includes(m));
    const ids = new Set(ms.map((m) => m.id));
    const offPlan = p.cards.filter((c) => c.doc && !ids.has(c.doc.milestone));
    const outside = (Array.isArray(plan.outside) ? plan.outside : []).map((it) => typeof it === "string" ? { text: it } : it).filter(isObj);
    const COLOR = { wait: t.blocked, block: t.ansi1, working: t.working, done: t.done, idle: t.dim };
    const chip = (c) => div().id(`rc-${c.terminal}`).flex_none().px(5).rounded(4).border_1().border_color(COLOR[c.column]).text_color(COLOR[c.column])
      .text_size(11).cursor_pointer().on_click(() => this.go(c)).child(`${COLUMNS.find((k) => k.id === c.column).sym} ${titleOf(c).split(" · ")[0]}`);
    const bar = (d, n, w) => h_flex().w(w).h(4).rounded(2).bg(t.border).overflow_hidden()
      .child(div().h_full().w(n ? Math.round(w * d / n) : 0).bg(t.accent));
    const MARKC = { done: t.done, current: t.accent, pending: t.muted, wait: t.blocked, block: t.ansi1, info: t.ansi4 };
    const name = clean(plan.title || "Roadmap");
    return {
      mini: h_flex().gap(6).items_center().text_color(t.dim)
        .child(div().child(name)).child(bar(D, T, 70)).child(div().child(`${D}/${T}`))
        .when(started.length > 0, (el) => el.child(div().child(`· now ${started.map((m) => m.id).join(", ")}`))),
      body: () => {
        const box = v_flex().gap(10);
        box.child(h_flex().gap(12).items_center()
          .when(head.length > 0, (el) => el.child(div().text_color(t.done).font_semibold()
            .child(`✓ ${head.map((m) => m.id).join(" · ")} · ${head.reduce((a, m) => a + progress(m)[1], 0)} deliverables`)))
          .child(div().flex_1())
          .child(div().text_color(t.dim).child(`${msDone}/${ms.length} milestones · ${D}/${T} deliverables`)));
        if (started.length) box.child(h_flex().flex_wrap().items_start().gap(8).children(started.map((m) => {
          const [d, n] = progress(m);
          const attached = here(m);
          const its = items(m);
          return v_flex().id(`ms-${p.root}-${m.id}`).w(440).flex_grow(1).min_w(0).p(10).gap(6).rounded(8).border_1().border_color(t.accent).bg(t.surface)
            .child(h_flex().gap(8).items_center()
              .child(div().font_bold().child(clean(m.id)))
              .child(div().flex_1().min_w(0).truncate().font_semibold().child(clean(m.title ?? "")))
              .child(bar(d, n, 60)).child(div().text_color(t.dim).child(`${d}/${n}`)))
            .when(Boolean(m.done_when), (el) => el.child(div().text_size(11).text_color(t.dim).child(`Done when: ${clean(m.done_when)}`)))
            .children((() => {
              // A long milestone stays short: what is done folds into one line, and only the first
              // few open deliverables show — plus any a console works on — until it is opened.
              const key = `${p.root}:${m.id}`, whole = Boolean(this.msFull[key]);
              const onIt = (it) => attached.some((c) => it.id && c.doc.deliverable === it.id);
              const doneN = its.filter((it) => it.mark === "done").length;
              let open = 0;
              const shown = its.map((it, i) => [it, i]).filter(([it]) => {
                if (whole || onIt(it)) return true;
                if (it.mark === "done") return false;
                return open++ < 5;
              });
              const hidden = its.length - shown.length;
              const toggle = (label) => div().id(`mt-${key}`).text_size(11).text_color(t.accent).cursor_pointer()
                .on_click((_e, cx) => { this.msFull[key] = !whole; cx.notify(); }).child(label);
              const out = [];
              if (!whole && doneN > 0) out.push(div().text_color(t.done).child(`✓ ${doneN} done`));
              out.push(...shown.map(([it, i]) => this.msItem(p, m, it, i, attached, t, chip, MARKC)));
              if (hidden > 0) out.push(toggle(`Show all ${its.length}`));
              else if (whole) out.push(toggle("Show less"));
              return out;
            })())
            .when(attached.some((c) => !its.some((it) => it.id && it.id === c.doc.deliverable)), (el) => el.child(h_flex().flex_wrap().gap(4)
              .children(attached.filter((c) => !its.some((it) => it.id && it.id === c.doc.deliverable)).map(chip))));
        })));
        if (later.length) box.child(h_flex().flex_wrap().gap(6).children(later.map((m) => {
          const [d, n] = progress(m);
          const deps = Array.isArray(m.depends_on) && m.depends_on.length ? ` · after ${m.depends_on.join(", ")}` : "";
          return h_flex().gap(6).px(8).py(3).rounded(6).border_1().border_color(t.border).bg(t.surface)
            .child(div().font_bold().text_color(n && d === n ? t.done : t.foreground).child(`${n && d === n ? "✓ " : ""}${clean(m.id)}`))
            .child(div().child(clean(m.title ?? "")))
            .child(div().text_color(t.dim).child(`${d}/${n}${deps}`));
        })));
        return this.rmTail(box, offPlan, outside, plan, chip, t);
      },
    };
  }

  msItem(p, m, it, i, attached, t, chip, MARKC) {
    const on = attached.filter((c) => it.id && c.doc.deliverable === it.id);
    return h_flex().id(`mi-${p.root}-${m.id}-${i}`).gap(6).items_start()
      .child(div().w(12).flex_none().text_color(MARKC[it.mark] ?? t.dim).child(MARKS[it.mark] ?? "·"))
      .child(div().flex_1().min_w(0).text_color(it.mark === "done" ? t.dim : t.foreground).child(clean(it.text)))
      .when(Boolean(it.ref), (el) => el.child(div().flex_none().text_color(t.dim).child(clean(it.ref))))
      .children(on.map(chip));
  }

  rmTail(box, offPlan, outside, plan, chip, t) {
    if (offPlan.length) box.child(h_flex().flex_wrap().gap(6).items_center()
      .child(div().text_color(t.dim).child("Off the plan:")).children(offPlan.map(chip)));
    if (outside.length) box.child(h_flex().flex_wrap().gap(6).items_center()
      .child(div().text_color(t.dim).child(`Outside the code · ${outside.filter((o) => o.mark !== "done").length} pending:`))
      .children(outside.map((o) => div().px(8).py(2).rounded(6).border_1().border_color(t.border)
        .text_color(o.mark === "done" ? t.dim : t.foreground).child(`${o.mark === "done" ? "✓ " : ""}${clean(o.text)}`))));
    if (plan.source) box.child(div().text_size(11).text_color(t.dim).child(`From ${clean(plan.source)}`));
    return box;
  }

  // ─── The board ───────────────────────────────────────────────────────────────────────────────

  render() {
    const t = theme();
    const where = {};
    for (const s of spaces()) for (const tab of s.tabs) where[tab.id] = { space: s.id };
    const now = Date.now();
    const cards = panes().filter((p) => p.terminal).map((p) => {
      const doc = this.docs[p.terminal], mtime = this.mtimes[p.terminal];
      const c = {
        terminal: p.terminal, tab: p.tab, status: p.status, cwd: p.cwd, branch: p.branch,
        program: p.program?.name ?? p.title, ...where[p.tab], doc, mtime,
        since: Number.isFinite(doc?.attention_since) ? doc.attention_since * 1000 : null,
      };
      c.stale = Boolean(doc && mtime && doc.attention !== "wait" && doc.attention !== "done"
        && now - mtime > (Number(doc.stale_after_minutes) || 30) * 60000);
      c.column = columnOf(c);
      c.root = (p.cwd && this.roots[p.cwd]) || null;
      return c;
    });

    const groups = {};
    for (const c of cards) {
      const key = c.root ?? c.cwd ?? "—";
      (groups[key] ??= { root: key, name: c.root ? c.root.split("/").pop() : (c.cwd === home() ? "~" : (c.cwd?.split("/").pop() ?? "—")), plan: c.root ? this.plans[c.root] : null, cards: [] }).cards.push(c);
    }
    const best = (g) => Math.min(...g.cards.map((c) => RANK[c.column]));
    const projects = Object.values(groups).sort((a, b) => best(a) - best(b) || a.name.localeCompare(b.name));
    const counts = {};
    for (const c of cards) counts[c.column] = (counts[c.column] ?? 0) + 1;
    const COLOR = { wait: t.blocked, block: t.ansi1, working: t.working, done: t.done, idle: t.dim };
    const roadmaps = projects.map((p) => [p, this.roadmap(p, t)]);
    const withRoadmap = roadmaps.filter(([, r]) => r);
    const allOpen = withRoadmap.length > 0 && withRoadmap.every(([p]) => this.rmOpen[p.root]);

    const cell = (k) => v_flex().flex_basis(0).flex_grow(k.grow).min_w(0).p(8).gap(8);
    const board = v_flex().min_w(1360).w_full();
    board.child(h_flex().w_full().border_b(1).border_color(t.border).children(COLUMNS.map((k) =>
      h_flex().flex_basis(0).flex_grow(k.grow).min_w(0).px(10).py(8).gap(6).items_center().text_color(COLOR[k.id]).font_semibold()
        .child(div().child(k.sym)).child(div().truncate().child(k.name.toUpperCase()))
        .child(div().flex_1())
        .child(div().px(7).rounded(9).bg(t.raised).text_color(counts[k.id] ? COLOR[k.id] : t.dim).child(String(counts[k.id] ?? 0))))));

    for (const [p, rm] of roadmaps) {
      const open = Boolean(rm && this.rmOpen[p.root]);
      board.child(h_flex().w_full().px(12).py(8).gap(12).items_center().bg(t.raised).border_b(1).border_color(t.border)
        .child(div().font_bold().text_size(13).child(p.name))
        .child(div().text_color(t.dim).child(`${p.cards.length} console${p.cards.length === 1 ? "" : "s"}`))
        .child(div().flex_1())
        .child(rm ? rm.mini : div().text_color(t.dim).child("no roadmap"))
        .when(Boolean(rm), (el) => el.child(h_flex().id(`rm-${p.root}`).gap(6).px(8).py(2).rounded(6).border_1().border_color(t.border).bg(t.surface)
          .cursor_pointer().hover((s) => s.bg(t.hover))
          .on_click((_e, cx) => { this.rmOpen[p.root] = !open; cx.notify(); })
          .child(div().child(open ? "▾" : "▸")).child(div().child("Roadmap")))));
      if (open) board.child(div().w_full().px(12).py(10).border_b(1).border_color(t.border).child(rm.body()));
      board.child(h_flex().w_full().items_start().border_b(1).border_color(t.border).children(COLUMNS.map((k) => {
        const here = p.cards.filter((c) => c.column === k.id);
        if (k.id === "wait") here.sort((a, b) => (a.since ?? Infinity) - (b.since ?? Infinity));
        return cell(k).children(here.length ? here.map((c) => this.card(c, t)) : [div().text_color(t.border).child("—")]);
      })));
    }

    const mono = settings().font_family ?? "JetBrains Mono";
    return v_flex().id("bellwether-board").size_full().min_h(0).bg(t.canvas).text_color(t.foreground).text_size(12).font_family(mono)
      .child(h_flex().h(46).flex_none().px(16).gap(10).items_center().border_b(1).border_color(t.border)
        .child(div().font_semibold().text_size(14).child("Board"))
        .child(div().text_color(t.dim).child(`${cards.length} consoles · ${projects.length} projects`))
        .child(div().flex_1())
        .when(withRoadmap.length > 0, (el) => el.child(div().id("rm-all").px(9).py(3).rounded(6).border_1().border_color(t.border).bg(t.surface)
          .cursor_pointer().hover((s) => s.bg(t.hover))
          .on_click((_e, cx) => { for (const [p] of withRoadmap) this.rmOpen[p.root] = !allOpen; cx.notify(); })
          .child(allOpen ? "Close all roadmaps" : "Open all roadmaps"))))
      .child(div().id("board-body").flex_1().min_h(0).overflow_scroll().child(board));
  }
}
