import {
  type Cell,
  type IconSize,
  type Placement,
  type Section,
  TRACKER_COLUMNS,
} from "@/bindings";

/** Each section's title, as the grid and its toolbars show it. */
export const TITLES: Record<Section, string> = {
  Inventory: "Inventory",
  Equipment: "Equipment",
  Upgrades: "Upgrades",
  Quest: "Quest",
  Songs: "Songs",
  Stones: "Spiritual Stones",
  Medallions: "Medallions",
  Dungeons: "Dungeons",
};

/** How a toolbar button changes a section's place or width. */
export type Move =
  | "left"
  | "right"
  | "up"
  | "down"
  | "narrower"
  | "wider"
  | "hide"
  | "show";

/** The sections after one move, or null when that move can't go further.
 * `slots` caps how wide the section can get. */
export function move(
  sections: Placement[],
  section: Section,
  m: Move,
  slots: number,
): Placement[] | null {
  const at = sections.findIndex((p) => p.section === section);
  const p = sections[at];
  if (!p) return null;
  const set = (patch: Partial<Placement>) =>
    sections.map((o, i) => (i === at ? { ...o, ...patch } : o));
  // Up and down skip hidden sections, so every press shows a change.
  const column = sections.flatMap((o, i) =>
    o.column === p.column && (o.shown || i === at) ? [i] : [],
  );
  const k = column.indexOf(at);
  const swap = (to: number | undefined) =>
    to === undefined
      ? null
      : sections.map((o, i) =>
          i === at ? (sections[to] ?? o) : i === to ? p : o,
        );
  switch (m) {
    case "left":
      return p.column > 1 ? set({ column: p.column - 1 }) : null;
    case "right":
      return p.column < TRACKER_COLUMNS ? set({ column: p.column + 1 }) : null;
    case "up":
      return swap(column[k - 1]);
    case "down":
      return swap(column[k + 1]);
    case "narrower":
      return p.width > 1 ? set({ width: p.width - 1 }) : null;
    case "wider":
      return p.width < slots ? set({ width: p.width + 1 }) : null;
    case "hide":
      return set({ shown: false });
    case "show":
      return set({ shown: true });
  }
}

/** A tile's side in pixels, by size. */
export const PIXELS: Record<IconSize, number> = {
  Small: 32,
  Medium: 48,
  Large: 72,
};

/** The count badge's look, on a tile or as its own button. */
export const BADGE =
  "absolute -right-0.5 -bottom-0.5 inline-flex h-4 min-w-4.5 items-center justify-center rounded-xs bg-ink px-1 font-mono font-semibold text-[10.5px] text-surface leading-none";

/** The sections shown in each column, empty columns left out. */
export function columnsOf(sections: Placement[], count: number) {
  return Array.from({ length: count }, (_, i) => ({
    n: i + 1,
    shown: sections.filter((p) => p.shown && p.column === i + 1),
  })).filter((c) => c.shown.length > 0);
}

/** One section's cells, each with where it sits among all of them. */
export const inSection = (cells: Cell[], section: Section) =>
  cells.flatMap((cell, at) => (cell.section === section ? [{ cell, at }] : []));
