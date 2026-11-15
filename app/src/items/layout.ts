import { type Placement, type Section, TRACKER_COLUMNS } from "@/bindings";

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
