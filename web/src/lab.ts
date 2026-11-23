// The lab's public state: written by the lab every minute, read by every part of the page.

export interface LabRecord {
  name: string;
  v: number;
  k: number;
  t: number;
  value: number;
  previous: number;
  previousSince: number;
  lowerBound: number;
  pairs: number;
  closed: string;
  stamped: string;
  sha256: string;
  certificate: string;
  proof: string;
}

export interface Idea {
  n: number;
  model: string;
  short: string;
  status: string;
  delta: number | null;
  utc: string;
}

export interface Adopted {
  n: number;
  model: string;
  idea: string;
  short: string;
  utc: string;
  before: number;
  after: number;
  diff: string[];
}

/** One model's turns in the loop: answers given, changes that built and ran, changes kept, best score gain. */
export interface LeagueRow {
  model: string;
  tier: "fast" | "frontier";
  turns: number;
  ran: number;
  adopted: number;
  best: number | null;
  spentUsd: number;
}

export interface Lab {
  updated: string;
  record: LabRecord;
  records: LabRecord[];
  hunting: { name: string; table: number; target: number; open: number }[];
  /** Searches running right now: the entry, its table value, the level being tried and the sets still open there. */
  live?: { name: string; t: number; table: number; best: number | null; level: number | null; open: number | null; seconds: number }[];
  ideas: Idea[];
  adopted: Adopted[];
  league?: LeagueRow[];
  today: { ideas: number; adopted: number; spentUsd: number };
  total: { ideas: number; adopted: number; spentUsd: number; generation: number };
}

/** Where the live state is read: the site's data route in production, a local file while developing. */
export const DATA_URL: string = import.meta.env.VITE_DATA_URL ?? (import.meta.env.PROD ? "/data/lab.json" : "/lab.json");

export async function loadLab(): Promise<Lab> {
  for (const url of [DATA_URL, "/lab.json"]) {
    const response = await fetch(url, { cache: "no-store" }).catch(() => null);
    if (response?.ok) return (await response.json()) as Lab;
  }
  throw new Error("the lab's state could not be loaded");
}

export const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
export const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
