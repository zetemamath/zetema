import "./style.css";
import { startFano } from "./fano";
import { startHero } from "./hero";
import { loadLab } from "./lab";
import { refreshSections, renderSections } from "./sections";

async function main(): Promise<void> {
  const lab = await loadLab();
  renderSections(lab);
  startFano();
  await startHero(lab);
  window.setInterval(async () => {
    if (document.hidden) return;
    try {
      refreshSections(await loadLab());
    } catch {
      // The board keeps what it last knew; the next minute tries again.
    }
  }, 60_000);
}

void main();
