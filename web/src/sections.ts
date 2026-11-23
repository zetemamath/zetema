// Everything below the board: the ledger of records, the lab's notebook and the checker anyone can use.

import { Chalk, handwrite } from "./chalk";
import { check, explain } from "./check";
import { font, measure } from "./hero";
import { reduced, type Lab } from "./lab";

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
          el("p", { className: "label", textContent: `adopted \u00b7 idea ${a.n} \u00b7 Claude ${a.model} \u00b7 ${a.utc.replace(" ", " at ")} UTC` }),
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

export function renderSections(lab: Lab): void {
  ledger(lab);
  notebook(lab);
  checker(lab);
  chalkTitles();
}

export function refreshSections(lab: Lab): void {
  ledger(lab);
  notebook(lab);
}
