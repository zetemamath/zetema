// Chalk on a board: handwriting laid out from a single-stroke font, drawn stroke by stroke.
//
// Strokes are painted solid into an offscreen mask, and the visible canvas shows a speckled chalk grain through
// that mask. Overlapping strokes therefore never darken into beads, and erasing is just lowering the mask.

export interface HandFont {
  unitsPerEm: number;
  glyphs: Record<string, { a: number; s: number[][] }>;
}

/** One pen stroke in CSS pixels: x0, y0, x1, y1, ... */
export type Polyline = number[];

export interface Written {
  lines: Polyline[];
  width: number;
}

function random(seed: number): () => number {
  let state = seed | 0;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hash(text: string): number {
  let h = 2166136261;
  for (const c of text) {
    h ^= c.codePointAt(0) ?? 0;
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** The hand font has only ASCII quotes and the hyphen, so typographic quotes, minus signs and dashes are written as those. */
export const plain = (text: string): string =>
  text.replace(/[\u2018\u2019]/g, "'").replace(/[\u201c\u201d]/g, '"').replace(/[\u2212\u2013]/g, "-");

/** Lays out text as handwriting: every glyph is turned, raised and sized a little differently, the same way each time. */
export function handwrite(font: HandFont, text: string, x: number, baseline: number, size: number): Written {
  const roll = random(hash(text));
  const k = size / font.unitsPerEm;
  const lines: Polyline[] = [];
  let pen = x;
  for (const ch of plain(text)) {
    const glyph = font.glyphs[ch] ?? font.glyphs["?"];
    if (!glyph) continue;
    const turn = (roll() - 0.5) * 0.07;
    const lift = (roll() - 0.5) * 0.035 * size;
    const grow = 1 + (roll() - 0.5) * 0.05;
    const [cx, cy, cos, sin] = [glyph.a / 2, 300, Math.cos(turn), Math.sin(turn)];
    for (const stroke of glyph.s) {
      const line: number[] = [];
      for (let i = 0; i < stroke.length; i += 2) {
        const ux = (stroke[i] - cx) * grow;
        const uy = (stroke[i + 1] - cy) * grow;
        line.push(pen + (ux * cos - uy * sin + cx) * k, baseline - (ux * sin + uy * cos + cy) * k + lift);
      }
      lines.push(line);
    }
    pen += glyph.a * k * (1 + (roll() - 0.5) * 0.05);
  }
  return { lines, width: pen - x };
}

/** A straight chalk line drawn with the side of the stick: slightly bowed, slightly uneven. */
export function rule(x0: number, y0: number, x1: number, y1: number, seed: number): Polyline {
  const roll = random(seed);
  const line: number[] = [];
  const bow = (roll() - 0.5) * 0.012 * Math.hypot(x1 - x0, y1 - y0);
  for (let i = 0; i <= 12; i++) {
    const t = i / 12;
    line.push(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t + Math.sin(Math.PI * t) * bow + (roll() - 0.5) * 0.6);
  }
  return line;
}

interface Piece {
  line: Polyline;
  width: number;
}

export class Chalk {
  private readonly ctx: CanvasRenderingContext2D;
  private readonly mask = document.createElement("canvas");
  private readonly ink: CanvasRenderingContext2D;
  private grain: CanvasPattern;
  private dirty = false;
  private ratio = 1;
  private size = { w: 0, h: 0 };

  constructor(private readonly canvas: HTMLCanvasElement) {
    this.ctx = canvas.getContext("2d")!;
    this.ink = this.mask.getContext("2d")!;
    this.grain = this.makeGrain();
  }

  /** Matches the canvas to its box. Clears everything. */
  resize(w: number, h: number): void {
    this.ratio = Math.min(window.devicePixelRatio || 1, 2);
    this.size = { w, h };
    for (const c of [this.canvas, this.mask]) {
      c.width = Math.round(w * this.ratio);
      c.height = Math.round(h * this.ratio);
    }
    this.canvas.style.width = `${w}px`;
    this.canvas.style.height = `${h}px`;
    for (const c of [this.ctx, this.ink]) c.setTransform(this.ratio, 0, 0, this.ratio, 0, 0);
    this.grain = this.makeGrain();
    this.render();
  }

  clear(): void {
    this.ink.clearRect(0, 0, this.size.w, this.size.h);
    this.schedule();
  }

  /** Draws strokes at once. */
  paint(lines: Polyline[], width: number): void {
    for (const line of lines) this.stroke(line, 0, Infinity, width);
    this.schedule();
  }

  /** Draws strokes as a hand would, at `speed` CSS pixels per millisecond, lifting the chalk between strokes. */
  write(lines: Polyline[], width: number, speed: number): Promise<void> {
    const pieces: Piece[] = lines.map((line) => ({ line, width }));
    const lengths = pieces.map((p) => length(p.line));
    const pause = 70 * speed;
    const total = lengths.reduce((sum, l) => sum + l + pause, 0);
    return new Promise((resolve) => {
      let start = 0;
      let done = 0;
      const frame = (now: number) => {
        if (!start) start = now;
        const reach = Math.min(total, (now - start) * speed);
        let at = 0;
        for (let i = 0; i < pieces.length; i++) {
          const [from, to] = [Math.max(done, at) - at, Math.min(reach, at + lengths[i]) - at];
          if (to > from) this.stroke(pieces[i].line, from, to, pieces[i].width);
          at += lengths[i] + pause;
        }
        done = reach;
        this.render();
        if (reach < total) requestAnimationFrame(frame);
        else resolve();
      };
      requestAnimationFrame(frame);
    });
  }

  /** Wipes a box the way a felt eraser does: in sweeps, leaving a faint ghost of what was there. */
  erase(x: number, y: number, w: number, h: number, ms: number): Promise<void> {
    const sweeps = Math.max(3, Math.round(h / 34));
    const band = h / sweeps;
    return new Promise((resolve) => {
      let start = 0;
      let wiped = 0;
      const frame = (now: number) => {
        if (!start) start = now;
        const t = Math.min(1, (now - start) / ms);
        const reach = t < 1 ? Math.floor(t * sweeps) : sweeps;
        this.ink.save();
        this.ink.globalCompositeOperation = "destination-out";
        this.ink.globalAlpha = 0.93;
        for (; wiped < reach; wiped++) this.ink.fillRect(x - 12, y + wiped * band - 7, w + 24, band + 14);
        this.ink.restore();
        this.render();
        if (t < 1) requestAnimationFrame(frame);
        else resolve();
      };
      requestAnimationFrame(frame);
    });
  }

  private stroke(line: Polyline, from: number, to: number, width: number): void {
    const c = this.ink;
    c.strokeStyle = "#fff";
    c.lineCap = "round";
    c.lineJoin = "round";
    let at = 0;
    for (let i = 2; i < line.length; i += 2) {
      const [x0, y0, x1, y1] = [line[i - 2], line[i - 1], line[i], line[i + 1]];
      const l = Math.hypot(x1 - x0, y1 - y0);
      const [a, b] = [Math.max(from, at), Math.min(to, at + l)];
      if (b > a && l > 0) {
        const [s, e] = [(a - at) / l, (b - at) / l];
        c.lineWidth = width * (0.86 + 0.28 * Math.abs(Math.sin(x0 * 0.13 + y0 * 0.07)));
        c.beginPath();
        c.moveTo(x0 + (x1 - x0) * s, y0 + (y1 - y0) * s);
        c.lineTo(x0 + (x1 - x0) * e, y0 + (y1 - y0) * e);
        c.stroke();
      }
      at += l;
      if (at > to) break;
    }
  }

  private schedule(): void {
    if (this.dirty) return;
    this.dirty = true;
    requestAnimationFrame(() => {
      this.dirty = false;
      this.render();
    });
  }

  private render(): void {
    const c = this.ctx;
    c.save();
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.clearRect(0, 0, this.canvas.width, this.canvas.height);
    c.drawImage(this.mask, 0, 0);
    c.globalCompositeOperation = "source-in";
    c.fillStyle = this.grain;
    c.fillRect(0, 0, this.canvas.width, this.canvas.height);
    c.restore();
  }

  private makeGrain(): CanvasPattern {
    const tile = document.createElement("canvas");
    tile.width = tile.height = 180;
    const g = tile.getContext("2d")!;
    const image = g.createImageData(180, 180);
    for (let i = 0; i < image.data.length; i += 4) {
      const hole = Math.random() < 0.2;
      image.data[i] = 239;
      image.data[i + 1] = 237;
      image.data[i + 2] = 230;
      image.data[i + 3] = hole ? 0 : 150 + Math.random() * 105;
    }
    g.putImageData(image, 0, 0);
    return this.ctx.createPattern(tile, "repeat")!;
  }
}

function length(line: Polyline): number {
  let sum = 0;
  for (let i = 2; i < line.length; i += 2) sum += Math.hypot(line[i] - line[i - 2], line[i + 1] - line[i - 1]);
  return sum;
}
