import "./style.css";
import { startFano } from "./fano";
import { refreshHero, startHero } from "./hero";
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
      const fresh = await loadLab();
      refreshSections(fresh);
      refreshHero(fresh);
    } catch {
      // The board keeps what it last knew; the next minute tries again.
    }
  }, 60_000);
}

void main();
