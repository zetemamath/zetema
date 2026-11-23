// The exact checker, compiled from the same Rust code as the command line tool, running in the browser.

export interface Verdict {
  ok: boolean;
  reason?: string;
  v: number;
  k: number;
  t: number;
  blocks: number;
  sets: number;
  uncovered: number;
  witness: number[];
  bound: number;
  sha256: string;
  ms: number;
}

interface Exports {
  memory: WebAssembly.Memory;
  zetema_alloc: (n: number) => number;
  zetema_check: (p: number, n: number) => number;
  zetema_answer: () => number;
}

let module: Promise<Exports> | null = null;

function load(): Promise<Exports> {
  module ??= fetch("/zetema.wasm")
    .then((r) => r.arrayBuffer())
    .then((bytes) => WebAssembly.instantiate(bytes, {}))
    .then(({ instance }) => instance.exports as unknown as Exports);
  return module;
}

/** Checks a certificate given as text. Throws only when the checker itself cannot be loaded. */
export async function check(text: string): Promise<Verdict> {
  const ex = await load();
  const start = performance.now();
  const input = new TextEncoder().encode(text);
  const at = ex.zetema_alloc(input.length);
  new Uint8Array(ex.memory.buffer, at, input.length).set(input);
  const n = ex.zetema_check(at, input.length);
  const answer = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ex.zetema_answer(), n)));
  return { ...answer, ms: performance.now() - start } as Verdict;
}

/** The verdict as short lines for a reader; `expected` is the digest the record was published with. */
export function explain(v: Verdict, expected?: string): string[] {
  if (v.reason) return [`not a certificate: ${v.reason}`];
  const name = `C(${v.v},${v.k},${v.t})`;
  const sets = v.t === 2 ? "pairs" : `${v.t}-sets`;
  const lines = [`${name}: ${v.blocks} blocks of ${v.k} points, ${v.v} points in all`];
  if (!v.ok) {
    lines.push(`${(v.sets - v.uncovered).toLocaleString("en")} of ${v.sets.toLocaleString("en")} ${sets} covered: not a covering`);
    lines.push(`for example {${v.witness.join(", ")}} lies in no block`);
    return lines;
  }
  lines.push(`all ${v.sets.toLocaleString("en")} ${sets} lie in a block \u2713`);
  lines.push(
    v.blocks === v.bound
      ? `no covering can use fewer than ${v.bound} blocks, so ${name} = ${v.blocks} exactly`
      : `so ${name} \u2264 ${v.blocks}; the lower bound is ${v.bound}`,
  );
  if (expected) lines.push(`sha256 ${v.sha256.slice(0, 12)}\u2026${v.sha256.slice(-6)} ${v.sha256 === expected ? "matches the record" : "does NOT match the record"}`);
  else lines.push(`sha256 ${v.sha256}`);
  lines.push(`checked by your browser in ${v.ms < 1 ? "under 1" : v.ms.toFixed(1)} ms`);
  return lines;
}
