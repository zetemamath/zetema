// Everything below the board: the ledger of records, the lab's notebook and the checker anyone can use.

import { Chalk, handwrite } from "./chalk";
import { check, explain } from "./check";
import { font, measure } from "./hero";
import { reduced, wait, type Lab } from "./lab";
import { sponsorSearch, tokens } from "./sponsor";

const $ = <T extends Element = HTMLElement>(selector: string, root: ParentNode = document) => root.querySelector<T>(selector)!;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Partial<HTMLElementTagNameMap[K]> = {}, ...children: (Node | string)[]) {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
}

const day = (iso: string) => new Date(iso).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric", timeZone: "UTC" });
const signed = (x: number) => `${x >= 0 ? "+" : "\u2212"}${Math.abs(x).toFixed(1)}`;

/** Section titles are written in chalk the first time they come into view. */
function chalkTitles(): void {
  const titles = document.querySelectorAll<HTMLElement>("[data-chalk-title]");
  const write = async (title: HTMLElement, animate: boolean) => {
    const size = parseFloat(getComputedStyle(title).fontSize);
    const text = title.textContent ?? "";
    const canvas = el("canvas", { className: "title-chalk" });
    canvas.setAttribute("aria-hidden", "true");
    title.append(canvas);
    const chalk = new Chalk(canvas);
    const width = measure(text, size) + size * 0.4;
    chalk.resize(width, size * 1.45);
    title.classList.add("chalked");
    const written = handwrite(font, text, size * 0.1, size * 1.05, size);
    if (animate) await chalk.write(written.lines, size * 0.075, size * 0.045);
    else chalk.paint(written.lines, size * 0.075);
  };
  if (reduced || !("IntersectionObserver" in window)) {
    titles.forEach((t) => void write(t, false));
    return;
  }
  const seen = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        seen.unobserve(entry.target);
        void write(entry.target as HTMLElement, true);
      }
    },
    { rootMargin: "0px 0px -15% 0px" },
  );
  titles.forEach((t) => seen.observe(t));
}

function ledger(lab: Lab): void {
  const body = $("[data-ledger]");
  body.replaceChildren(
    ...lab.records.map((r) => {
      const status = r.value === r.lowerBound ? "exact, closed" : "new upper bound";
      const result = el("span", { className: "row-check" });
      const button = el("button", { type: "button", className: "mini", textContent: "check" });
      button.addEventListener("click", async () => {
        button.disabled = true;
        const verdict = await check(await fetch(`/${r.certificate}`).then((x) => x.text()));
        result.textContent = verdict.ok && verdict.sha256 === r.sha256 ? `\u2713 ${verdict.sets.toLocaleString("en")} pairs, ${verdict.ms.toFixed(1)} ms` : "does not check";
        result.classList.toggle("bad", !(verdict.ok && verdict.sha256 === r.sha256));
        button.disabled = false;
      });
      return el(
        "tr",
        {},
        el("th", { scope: "row" }, el("span", { className: "math", textContent: r.name })),
        el("td", {}, el("span", { className: "math dim", textContent: String(r.previous) }), ` since ${r.previousSince}`),
        el("td", {}, el("span", { className: "math red", textContent: String(r.value) })),
        el("td", {}, el("span", { className: "math", textContent: String(r.lowerBound) })),
        el("td", { textContent: status }),
        el("td", { textContent: day(r.closed) }),
        el(
          "td",
          { className: "proofs" },
          button,
          el("a", { href: `/${r.certificate}`, download: "", textContent: "blocks" }),
          el("a", { href: `/${r.proof}`, download: "", textContent: "timestamp" }),
          ...(r.chainTx && lab.chain ? [el("a", { href: `${lab.chain.explorer}/tx/${r.chainTx}`, target: "_blank", rel: "noopener", textContent: "on chain" })] : []),
          result,
        ),
      );
    }),
  );
  $("[data-near]").replaceChildren(
    ...lab.hunting.slice(0, 8).map((h) =>
      el(
        "li",
        {},
        el("span", { className: "math", textContent: h.name }),
        el("span", { className: "near-step" }, `${h.table} `, el("span", { className: "arrow", textContent: "\u2192" }), ` ${h.target}?`),
        el("span", { className: "near-left", textContent: `${h.open} ${h.open === 1 ? "pair" : "pairs"} left` }),
      ),
    ),
  );
}

function notebook(lab: Lab): void {
  const t = lab.total;
  $("[data-stats]").replaceChildren(
    ...[
      [String(t.ideas), "ideas tested"],
      [String(t.adopted), "adopted"],
      [`$${t.spentUsd.toFixed(2)}`, "spent on models"],
      [String(t.generation), "generation in use"],
    ].map(([n, label]) => el("div", {}, el("span", { className: "math big", textContent: n }), el("span", { className: "label", textContent: label }))),
  );
  $("[data-adopted]").replaceChildren(
    ...lab.adopted
      .slice()
      .reverse()
      .map((a) =>
        el(
          "article",
          { className: "adopted" },
          el("p", { className: "label", textContent: `adopted \u00b7 idea ${a.n} \u00b7 ${a.model} \u00b7 ${a.utc.replace(" ", " at ")} UTC` }),
          el("blockquote", { textContent: a.idea }),
          el(
            "pre",
            { className: "diff" },
            ...a.diff.map((line) => el("span", { className: line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : line.startsWith("@@") ? "hunk" : "", textContent: `${line}\n` })),
          ),
          el("p", { className: "score" }, "score over four sets of seeds ", el("span", { className: "math", textContent: `${a.before.toFixed(1)} \u2192 ${a.after.toFixed(1)}` }), " (higher is better)"),
        ),
      ),
  );
  $("[data-league]").replaceChildren(
    ...(lab.league ?? []).map((r) =>
      el(
        "tr",
        {},
        el("td", {}, r.model, el("span", { className: "tier", textContent: r.tier })),
        el("td", { className: "math", textContent: String(r.turns) }),
        el("td", { className: "math", textContent: String(r.ran) }),
        el("td", { className: r.adopted ? "math kept" : "math", textContent: String(r.adopted) }),
        el("td", { className: "math", textContent: r.best === null ? "–" : signed(r.best) }),
        el("td", { className: "math", textContent: `$${r.spentUsd.toFixed(2)}` }),
      ),
    ),
  );
  $("[data-log]").replaceChildren(
    ...lab.ideas
      .slice()
      .reverse()
      .slice(0, 24)
      .map((i) =>
        el(
          "tr",
          { className: i.status === "adopted" ? "won" : "" },
          el("td", { className: "math", textContent: String(i.n) }),
          el("td", { textContent: i.model }),
          el("td", { className: "idea", textContent: i.short }),
          el("td", { className: "math", textContent: i.delta === null ? "\u2013" : signed(i.delta) }),
          el("td", { textContent: i.status }),
        ),
      ),
  );
  $("[data-updated]").textContent = `updated ${new Date(lab.updated).toUTCString().slice(5, 22)} UTC`;
}

function checker(lab: Lab): void {
  const box = $<HTMLTextAreaElement>("[data-cert]");
  const out = $("[data-cert-out]");
  const run = async () => {
    out.hidden = false;
    out.replaceChildren(el("p", { textContent: "checking \u2026" }));
    try {
      const verdict = await check(box.value);
      const known = lab.records.find((r) => r.sha256 === verdict.sha256);
      out.replaceChildren(...explain(verdict, known?.sha256).map((line) => el("p", { className: line.endsWith("\u2713") ? "ok" : "", textContent: line })));
      if (known) out.append(el("p", { textContent: `this is the certificate of the lab's record ${known.name} = ${known.value}` }));
    } catch (error) {
      out.replaceChildren(el("p", { textContent: `the checker could not run here: ${String(error)}` }));
    }
  };
  $("[data-cert-check]").addEventListener("click", () => void run());
  $("[data-cert-load]").addEventListener("click", async () => {
    box.value = await fetch(`/${lab.record.certificate}`).then((x) => x.text());
    void run();
  });
  box.addEventListener("dragover", (e) => e.preventDefault());
  box.addEventListener("drop", async (e) => {
    e.preventDefault();
    const file = e.dataTransfer?.files[0];
    if (!file || file.size > 2_000_000) return;
    box.value = await file.text();
    void run();
  });
}

const entry = (name: string) => name.match(/\d+/g)!.map(Number) as [number, number, number];
const wallet = (a: string) => `${a.slice(0, 6)}…${a.slice(-4)}`;

/** Who burned for what, and how each search went. */
function sponsoredList(lab: Lab): void {
  const jobs = lab.sponsored ?? [];
  const say = (j: NonNullable<Lab["sponsored"]>[number]) =>
    j.state === "running" ? `searching, ${j.leftHours} h left${j.open != null ? `, ${j.open} ${j.open === 1 ? "pair" : "pairs"} short of ${j.level}` : ""}`
      : j.state === "queued" ? "waiting for a free core"
        : j.state === "done" ? (j.found ? `found ${j.found} ✓` : "hours used, not broken yet")
          : j.note;
  $("[data-sponsored]").replaceChildren(
    ...(jobs.length
      ? jobs.map((j) =>
        el(
          "li",
          {},
          el("span", { className: "math", textContent: j.name }),
          el("a", { href: `${lab.chain?.explorer}/tx/${j.tx}`, target: "_blank", rel: "noopener", className: "near-step", textContent: `${wallet(j.sponsor)} · ${j.hours} h` }),
          el("span", { className: j.found ? "near-left red" : "near-left", textContent: say(j) }),
        ))
      : [el("li", { className: "empty", textContent: "no sponsored search yet" })]),
  );
}

/** The burn panel: an open record, hours, and the visitor's wallet signs. Hidden until the contract is live. */
function sponsorPanel(lab: Lab): void {
  const c = lab.chain;
  const section = $("[data-sponsor]");
  section.hidden = !(c?.sponsor && c.token && c.unit);
  if (section.hidden || !c?.sponsor) return;
  const pick = $<HTMLSelectElement>("[data-sponsor-entry]"), hours = $<HTMLInputElement>("[data-sponsor-hours]");
  const keep = pick.value;
  const open = new Map<string, string>();
  for (const h of lab.hunting) open.set(h.name, `${h.name}: ${h.table} → ${h.target}?`);
  for (const s of lab.live ?? []) if (!open.has(s.name)) open.set(s.name, `${s.name}: ${s.table} → ${s.table - 1}?`);
  pick.replaceChildren(...[...open].map(([name, label]) => el("option", { value: name, textContent: label })));
  if (open.has(keep)) pick.value = keep;
  hours.max = String(c.maxHours ?? 24);
  const cost = () => {
    const n = Math.max(1, Math.min(Number(hours.max), Math.floor(Number(hours.value) || 1)));
    $("[data-sponsor-cost]").textContent = `burns ${tokens(BigInt(c.unit!) * BigInt(n))} $${c.ticker ?? "ZETEMA"}`;
    return n;
  };
  cost();
  const link = $<HTMLAnchorElement>("[data-sponsor-contract]");
  link.href = `${c.explorer}/address/${c.sponsor}`;
  if (section.dataset.wired) return;
  section.dataset.wired = "1";
  hours.addEventListener("input", cost);
  const go = $<HTMLButtonElement>("[data-sponsor-go]"), status = $("[data-sponsor-status]");
  go.addEventListener("click", async () => {
    go.disabled = true;
    status.classList.remove("bad");
    try {
      const [v, k, t] = entry(pick.value);
      const hash = await sponsorSearch(c, v, k, t, cost(), (s) => (status.textContent = s));
      status.replaceChildren(`burned. The lab picks it up within a minute: `, el("a", { href: `${c.explorer}/tx/${hash}`, target: "_blank", rel: "noopener", textContent: "the transaction" }));
    } catch (error) {
      status.textContent = (error as { message?: string }).message ?? String(error);
      status.classList.add("bad");
    } finally {
      go.disabled = false;
    }
  });
}

/** The token's address on the board once it exists. One that shows up while the page is open types itself in. */
function contract(lab: Lab, live: boolean): void {
  const c = lab.chain;
  const box = $("[data-contract]");
  box.hidden = !c?.token;
  if (!c?.token) return;
  const address = $("[data-contract-address]");
  $("[data-contract-ticker]").textContent = `$${c.ticker ?? "ZETEMA"}`;
  $<HTMLAnchorElement>("[data-contract-explorer]").href = `${c.explorer}/token/${c.token}`;
  $("[data-contract-burn]").hidden = !c.sponsor;
  if (address.dataset.full !== c.token) {
    const typeIn = live && !address.dataset.full && !reduced;
    address.dataset.full = c.token;
    if (typeIn) void (async (text) => {
      for (let i = 1; i <= text.length && address.dataset.full === text; i++) {
        address.textContent = text.slice(0, i);
        await wait(24);
      }
    })(c.token);
    else address.textContent = c.token;
  }
  const copy = $<HTMLButtonElement>("[data-contract-copy]");
  if (copy.dataset.wired) return;
  copy.dataset.wired = "1";
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(address.dataset.full ?? "");
      copy.textContent = "copied ✓";
      copy.classList.add("done");
      await wait(1600);
      copy.textContent = "copy";
      copy.classList.remove("done");
    } catch {
      getSelection()?.selectAllChildren(address); // no clipboard here: leave it selected for a manual copy
    }
  });
}

export function renderSections(lab: Lab): void {
  contract(lab, false);
  ledger(lab);
  sponsorPanel(lab);
  sponsoredList(lab);
  notebook(lab);
  checker(lab);
  chalkTitles();
}

export function refreshSections(lab: Lab): void {
  contract(lab, true);
  ledger(lab);
  sponsorPanel(lab);
  sponsoredList(lab);
  notebook(lab);
}
