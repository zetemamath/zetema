// The smallest interesting covering, to play with: seven points, seven lines of three, every pair on one line.

import { reduced } from "./lab";

const figure = document.querySelector<SVGSVGElement>("[data-fano]");
const caption = document.querySelector<HTMLElement>("[data-fano-caption]");

export function startFano(): void {
  if (!figure || !caption) return;
  const points = [...figure.querySelectorAll<SVGGElement>("[data-point]")];
  const lines = [...figure.querySelectorAll<SVGElement>("[data-block]")];
  const blockOf = (a: string, b: string) => lines.find((l) => l.dataset.block!.split(" ").includes(a) && l.dataset.block!.split(" ").includes(b));
  let first: string | null = null;
  let touched = 0;

  const show = (a: string, b: string, byHand: boolean) => {
    const line = blockOf(a, b)!;
    for (const l of lines) l.classList.toggle("lit", l === line);
    for (const p of points) p.classList.toggle("lit", [a, b].includes(p.dataset.point!));
    const block = line.dataset.block!.split(" ").join(", ");
    caption.textContent = `${byHand ? "You picked" : "Points"} ${a} and ${b}: they lie together in the block {${block}}. Every one of the 21 pairs lies in exactly one of the 7 blocks.`;
  };

  for (const p of points) {
    p.addEventListener("click", () => {
      touched = Date.now();
      const id = p.dataset.point!;
      if (first === null || first === id) {
        first = id;
        for (const q of points) q.classList.toggle("picked", q === p);
        for (const l of lines) l.classList.remove("lit");
        caption.textContent = `Point ${id}. Now pick a second point.`;
        return;
      }
      show(first, id, true);
      for (const q of points) q.classList.remove("picked");
      first = null;
    });
  }

  if (reduced) return;
  let visible = false;
  new IntersectionObserver(([entry]) => (visible = entry.isIntersecting)).observe(figure);
  const ids = points.map((p) => p.dataset.point!);
  window.setInterval(() => {
    if (!visible || Date.now() - touched < 8000 || first !== null) return;
    const a = ids[Math.floor(Math.random() * ids.length)];
    let b = a;
    while (b === a) b = ids[Math.floor(Math.random() * ids.length)];
    show(a, b, false);
  }, 2400);
}
