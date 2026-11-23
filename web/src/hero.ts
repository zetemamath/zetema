// The top of the page: the latest record written on the board, and the lab's notes beside it.

import handFont from "./hand.json";
import { Chalk, handwrite, plain, rule, type HandFont, type Polyline } from "./chalk";
import { check, explain } from "./check";
import { reduced, wait, type Lab } from "./lab";

type Metrics = Record<string, { file: string; w: number; h: number; dy: number; digitHeight: number }>;

interface Layout {
  wide: boolean;
  size: number;
  x0: number;
  chalk: { lines: Polyline[]; width: number }[];
  pieces: { src: string; left: number; top: number; width: number }[];
  copyTop: number;
  notes: { left: number; top: number; width: number; size: number } | null;
}

export const font = handFont as HandFont;
const board = document.querySelector<HTMLElement>(".board")!;
const chalk = new Chalk(board.querySelector<HTMLCanvasElement>(".chalk")!);
const piecesBox = board.querySelector<HTMLElement>(".pieces")!;
const nowList = board.querySelector<HTMLElement>("[data-now]")!;
const clamp = (lo: number, x: number, hi: number) => Math.max(lo, Math.min(x, hi));

export function measure(text: string, size: number): number {
  let w = 0;
  for (const ch of plain(text)) w += (font.glyphs[ch] ?? font.glyphs["?"])?.a ?? 0;
  return (w * size) / font.unitsPerEm;
}

/** Splits text into lines no wider than `width` at the given chalk size. */
function wrap(text: string, size: number, width: number): string[] {
  const lines: string[] = [];
  let line = "";
  for (const word of text.split(" ")) {
    const next = line ? `${line} ${word}` : word;
    if (line && measure(next, size) > width) {
      lines.push(line);
      line = word;
    } else line = next;
  }
  if (line) lines.push(line);
  return lines;
}

function plan(lab: Lab, metrics: Metrics): Layout {
  const w = board.clientWidth;
  const h = window.innerHeight;
  const wide = w >= 900;
  const x0 = wide ? Math.max(56, (w - 1440) / 2 + 56) : 20;
  const size = wide ? clamp(66, Math.min(w * 0.074, (h - 260) * 0.24), 142) : clamp(34, (w - 40) / 6.7, 92);
  const top = board.querySelector<HTMLElement>(".top")!;
  const headerBottom = top.offsetTop + top.offsetHeight;
  const base = wide ? Math.max(headerBottom + 70 + size * 0.75, h * 0.32) : headerBottom + 34 + size * 0.75;
  const r = lab.record;
  const chalkLines: Layout["chalk"] = [];

  const name = handwrite(font, r.name, x0, base, size);
  chalkLines.push({ lines: name.lines, width: size * 0.082 });

  const digitH = size * 0.8;
  const k = digitH / metrics["0"].digitHeight;
  const pad = 6 * k;
  const redLeft = x0 + name.width + size * 0.34;
  let x = redLeft;
  const pieces: Layout["pieces"] = [];
  for (const ch of ["=", ...String(r.value)]) {
    const m = metrics[ch];
    pieces.push({ src: `/${m.file}`, left: x - pad, top: base - digitH + m.dy * k, width: m.w * k });
    x += (m.w - 12) * k + (ch === "=" ? digitH * 0.3 : digitH * 0.05);
  }
  const redRight = x - digitH * 0.05;
  const under = [
    rule(redLeft - size * 0.1, base + size * 0.2, redRight + size * 0.12, base + size * 0.215, 11),
    rule(redLeft - size * 0.08, base + size * 0.345, redRight + size * 0.1, base + size * 0.355, 12),
  ];
  chalkLines.push({ lines: under, width: size * 0.075 });

  const small = size * 0.3;
  const wx = x0 + size * 0.42;
  const y1 = base + size * 0.98;
  const bound = `\u2265 \u2308${r.v}/${r.k} \u00b7 \u2308${r.v - 1}/${r.k - 1}\u2309\u2309 = ${r.lowerBound}`;
  const line1 = handwrite(font, bound, wx, y1, small);
  const y2 = y1 + small * 1.55;
  const lead = handwrite(font, "\u2264 ", wx, y2, small);
  const old = handwrite(font, String(r.previous), wx + lead.width, y2, small);
  const since = handwrite(font, `  (since ${r.previousSince})`, wx + lead.width + old.width, y2, small * 0.86);
  const strike = rule(wx + lead.width - small * 0.08, y2 - small * 0.05, wx + lead.width + old.width + small * 0.08, y2 - small * 0.55, 13);
  chalkLines.push({ lines: [...line1.lines, ...lead.lines, ...old.lines, strike, ...since.lines], width: small * 0.1 });

  const eqRight = Math.max(redRight + size * 0.12, wx + line1.width);
  const notesLeft = Math.max(eqRight + size * 0.7, w * 0.62);
  const notesWidth = Math.min(520, w - notesLeft - Math.max(40, x0 * 0.45));
  const notes = wide && notesWidth >= 300 ? { left: notesLeft, top: Math.max(118, h * 0.16), width: notesWidth, size: clamp(19, size * 0.21, 24) } : null;
  return { wide, size, x0, chalk: chalkLines, pieces, copyTop: y2 + size * 0.62, notes };
}

function placePieces(layout: Layout, animate: boolean): Promise<void> {
  piecesBox.replaceChildren();
  const imgs = layout.pieces.map((p) => {
    const img = new Image();
    img.src = p.src;
    img.alt = "";
    img.decoding = "async";
    Object.assign(img.style, { left: `${p.left}px`, top: `${p.top}px`, width: `${p.width}px` });
    if (animate) img.className = "placing";
    piecesBox.append(img);
    return img;
  });
  if (!animate) return Promise.resolve();
  return Promise.all(imgs.map((img) => img.decode().catch(() => undefined))).then(async () => {
    for (const img of imgs) {
      img.className = "placed placing";
      requestAnimationFrame(() => img.classList.remove("placing"));
      await wait(110);
    }
    await wait(300);
  });
}

// ----- the lab, now: notes written in chalk, wiped and rewritten -----

function pages(lab: Lab): string[][] {
  const sets = (t: number, n: number) => (t === 2 ? (n === 1 ? "pair" : "pairs") : n === 1 ? "set" : "sets");
  const live = (lab.live ?? [])
    .filter((s) => s.level !== null && s.open !== null)
    .slice(0, 3)
    .map((s) => `${s.name}  ${s.table} \u2192 ${s.table - 1}?  ${s.open} ${sets(s.t, s.open!)} left${s.level === s.table - 1 ? "" : ` at ${s.level}`}`);
  const hunting = live.length ? live : lab.hunting.slice(0, 3).map((h) => `${h.name}  ${h.table} \u2192 ${h.target}?  ${h.open} ${sets(2, h.open)} left`);
  const t = lab.today;
  const adopted = lab.ideas.filter((i) => i.status === "adopted").at(-1);
  const tested = lab.ideas.filter((i) => i !== adopted);
  const idea = (i: Lab["ideas"][number]) => {
    const d = i.delta ?? 0;
    const score = `${d >= 0 ? "+" : "\u2212"}${Math.abs(d).toFixed(1)}`;
    return [`idea ${i.n} \u00b7 ${i.model}`, i.short, i.status === "adopted" ? `${score} \u2192 adopted \u2713` : `${score} \u2192 ${i.status}`];
  };
  const page = (k: number) => [
    live.length ? "searching now, live" : "searching now",
    ...hunting,
    "",
    `${t.ideas} AI ideas today, ${t.adopted} adopted`,
    "",
    ...(adopted ? idea(adopted) : []),
    "",
    ...(tested.length ? idea(tested[tested.length - 1 - (k % tested.length)]) : []),
  ];
  return [page(0), page(1), page(2)];
}

class Notes {
  private page = 0;
  private timer = 0;
  private busy = false;
  private generation = 0;
  constructor(
    private lab: Lab,
    private box: Layout["notes"],
  ) {}

  update(lab: Lab, box: Layout["notes"]): void {
    this.lab = lab;
    this.box = box;
  }

  private lines(text: string[]): { y: number; text: string; size: number }[] {
    const b = this.box!;
    const out: { y: number; text: string; size: number }[] = [];
    let y = b.top + 34 + b.size;
    for (const line of text) {
      if (!line) {
        y += b.size * 0.8;
        continue;
      }
      for (const part of wrap(line, b.size, b.width)) {
        out.push({ y, text: part, size: b.size });
        y += b.size * 1.42;
      }
    }
    return out;
  }

  /** Writes the current page. A newer call (after a resize, say) stops an older one between lines. */
  async show(animate: boolean): Promise<void> {
    const generation = ++this.generation;
    const all = pages(this.lab);
    const text = all[this.page % all.length];
    nowList.replaceChildren(...text.filter(Boolean).map((t) => Object.assign(document.createElement("li"), { textContent: t })));
    if (!this.box) return;
    for (const { y, text: t, size } of this.lines(text)) {
      if (generation !== this.generation) return;
      const written = handwrite(font, t, this.box.left, y, size);
      if (animate) await chalk.write(written.lines, size * 0.095, size * 0.06);
      else chalk.paint(written.lines, size * 0.095);
    }
  }

  private async next(): Promise<void> {
    if (this.busy || !this.box || document.hidden) return;
    this.busy = true;
    const b = this.box;
    await chalk.erase(b.left - 6, b.top + 30, b.width + 12, window.innerHeight * 0.62, 900);
    this.page++;
    await this.show(true);
    this.busy = false;
  }

  start(): void {
    clearInterval(this.timer);
    if (!this.box || reduced) return;
    this.timer = window.setInterval(() => void this.next(), 24000);
  }
}

async function verify(lab: Lab): Promise<void> {
  const out = board.querySelector<HTMLElement>("[data-verdict]")!;
  out.hidden = false;
  out.replaceChildren();
  const say = async (text: string, cls = "") => {
    const p = Object.assign(document.createElement("p"), { textContent: text, className: cls });
    out.append(p);
    if (!reduced) await wait(140);
  };
  const r = lab.record;
  await say(`reading ${r.certificate.split("/").pop()} and the 60 kB checker \u2026`);
  try {
    const text = await fetch(`/${r.certificate}`).then((x) => x.text());
    for (const line of explain(await check(text), r.sha256)) await say(line, line.endsWith("\u2713") ? "ok" : "");
  } catch (error) {
    await say(`the checker could not run here: ${String(error)}`);
    return;
  }
  const links = document.createElement("p");
  links.innerHTML = `<a href="/${r.certificate}" download>certificate</a> &middot; <a href="/${r.proof}" download>timestamp proof</a> &middot; <a href="#verify">check another</a>`;
  out.append(links);
}

let refresh: (lab: Lab) => void = () => {};

/** Hands the board the lab's newest state: the notes use it from their next page on, a phone's list at once. */
export const refreshHero = (lab: Lab): void => refresh(lab);

export async function startHero(lab: Lab): Promise<void> {
  const metrics = (await fetch("/numerals/metrics.json").then((r) => r.json())) as Metrics;
  const r = lab.record;
  board.querySelector("[data-equation]")!.textContent =
    `${r.name} = ${r.value}: ${r.value} blocks of ${r.k} points cover all ${r.pairs.toLocaleString("en")} pairs among ${r.v} points. The best known had been ${r.previous} since ${r.previousSince}, and no covering can use fewer than ${r.lowerBound}.`;
  const together = `every ${["two", "three", "four", "five"][r.t - 2] ?? r.t} of them`;
  board.querySelector("[data-lede]")!.textContent =
    `${r.v} people, meetings of ${r.k}, and ${together} must share a meeting. The best plan since ${r.previousSince} took ${r.previous} meetings. ` +
    `A search written by AI found one with ${r.value}${r.value === r.lowerBound ? ", and a proof shows no plan can use fewer." : "."}`;
  board.querySelector("[data-pulse]")!.textContent = `lab running \u00b7 ${lab.today.ideas} ideas today`;
  board.querySelector("[data-verify]")!.addEventListener("click", () => void verify(lab));
  await document.fonts.ready;

  let layout = plan(lab, metrics);
  const notes = new Notes(lab, layout.notes);
  refresh = (fresh) => {
    notes.update(fresh, layout.notes);
    board.querySelector("[data-pulse]")!.textContent = `lab running · ${fresh.today.ideas} ideas today`;
    if (!layout.notes) void notes.show(false);
  };
  const apply = (l: Layout) => {
    const top = board.querySelector<HTMLElement>(".top")!;
    board.style.setProperty("--x0", `${l.x0}px`);
    board.style.setProperty("--copy-top", `${Math.max(24, l.copyTop - (top.offsetTop + top.offsetHeight))}px`);
    if (l.notes) {
      board.style.setProperty("--now-left", `${l.notes.left}px`);
      board.style.setProperty("--now-top", `${l.notes.top}px`);
      board.style.setProperty("--now-w", `${l.notes.width}px`);
    }
    board.classList.toggle("chalked", Boolean(l.notes));
    chalk.resize(board.clientWidth, board.clientHeight);
  };

  apply(layout);
  if (reduced) {
    board.classList.remove("opening");
    for (const c of layout.chalk) chalk.paint(c.lines, c.width);
    await placePieces(layout, false);
    await notes.show(false);
  } else {
    const [name, under, working] = layout.chalk;
    await chalk.write(name.lines, name.width, layout.size * 0.034);
    await placePieces(layout, true);
    await chalk.write(under.lines, under.width, layout.size * 0.06);
    board.classList.remove("opening");
    await chalk.write(working.lines, working.width, layout.size * 0.05);
    await notes.show(true);
    notes.start();
  }

  let pending = 0;
  let lastWidth = board.clientWidth;
  window.addEventListener("resize", () => {
    clearTimeout(pending);
    pending = window.setTimeout(() => {
      if (board.clientWidth === lastWidth && !layout.wide) return; // phone toolbars change the height only
      lastWidth = board.clientWidth;
      layout = plan(lab, metrics);
      apply(layout);
      for (const c of layout.chalk) chalk.paint(c.lines, c.width);
      void placePieces(layout, false);
      notes.update(lab, layout.notes);
      void notes.show(false);
    }, 160);
  });
}
