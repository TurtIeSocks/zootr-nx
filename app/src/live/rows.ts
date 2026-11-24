import type { Change, Event, LiveStatus, Session, Snapshot } from "@/bindings";

/** Rows kept on screen. The session's log file keeps everything. */
export const CAP = 2000;

/** A value that changed, as the log shows it. */
export interface Row {
  /** The event's number, which orders rows across sessions. */
  id: number;
  /** When, in seconds, shown as `mm:ss` by {@link clock}. */
  at: number;
  /** It came from an autosave, not the live game. */
  save: boolean;
  change: Change;
  /** How many changes folded into it, when more than one. */
  count?: number;
}

/** A line that isn't a change: where a session starts or ends, or a
 * marker saying what happened. */
export interface Note {
  id: number;
  note: string;
  tone: "start" | "end" | "mark";
}

/** A line of the log. */
export type Item = Row | Note;

/** Whether the line is a change, not a session note. */
export const isRow = (i: Item): i is Row => "change" in i;

/** A value muted for changing too often. */
export interface Muted {
  hash: number;
  name: string;
}

/** The live store: the watcher's state and the log. */
export interface Live {
  status: LiveStatus;
  /** The newest session. */
  session?: Session;
  items: Item[];
  /** Items dropped off the top since the last Clear. */
  dropped: number;
  /** Muted this session. */
  muted: Muted[];
  /** Why the session isn't written to a file, or "" when it is. */
  unsaved: string;
  /** The next event's number. Anything below it is already here. */
  next: number;
}

/** The store before the watcher says anything. */
export const IDLE: Live = {
  status: { state: "Waiting", reason: null },
  items: [],
  dropped: 0,
  muted: [],
  unsaved: "",
  next: 0,
};

/** What changes the store: an event, a snapshot to replay, a Clear, or a
 * label saved. */
export type Action =
  | { type: "Event"; seq: number; event: Event }
  | { type: "Snapshot"; snapshot: Snapshot }
  | { type: "Cleared" }
  | { type: "Labelled"; hash: number; label: string };

/** Adds a line, dropping the oldest past {@link CAP}. */
function add(s: Live, item: Item): Live {
  const items = [...s.items, item];
  const over = Math.max(0, items.length - CAP);
  return { ...s, items: items.slice(over), dropped: s.dropped + over };
}

/** Applies one watcher event, numbered `id`. */
function apply(s: Live, id: number, e: Event): Live {
  switch (e.type) {
    case "Row":
      return add(s, { id, at: e.at, save: e.save, change: e.change });
    case "Muted": {
      const muted = [...s.muted, { hash: e.hash, name: e.name }];
      // A hand mute takes the value's rows too. The session log keeps them.
      const items = e.manual
        ? s.items.filter((i) => !isRow(i) || i.change.hash !== e.hash)
        : s.items;
      return { ...s, muted, items };
    }
    case "Marked":
      return add(s, { id, note: `${clock(e.at)} · ${e.text}`, tone: "mark" });
    case "Unsaved":
      return { ...s, unsaved: e.reason };
    case "Started": {
      const note = `Session started ${e.session.started}`;
      return add(
        {
          ...s,
          session: e.session,
          muted: [],
          unsaved: e.session.unsaved ?? "",
        },
        { id, note, tone: "start" },
      );
    }
    case "Ended":
      return add(s, { id, note: `Session ended: ${e.reason}`, tone: "end" });
    case "Status":
      return { ...s, status: e.status };
  }
}

/** The store after `a`. An event it already has is ignored. */
export function reduce(s: Live, a: Action): Live {
  switch (a.type) {
    case "Event":
      if (a.seq < s.next) return s;
      return { ...apply(s, a.seq, a.event), next: a.seq + 1 };
    case "Snapshot": {
      const { status, dropped, events, next } = a.snapshot;
      const replayed = events.reduce(
        (l, n) => reduce(l, { type: "Event", ...n }),
        { ...IDLE, dropped },
      );
      return { ...replayed, status, next };
    }
    case "Cleared":
      return { ...s, items: [], dropped: 0 };
    case "Labelled":
      return {
        ...s,
        items: s.items.map((r) =>
          isRow(r) && r.change.hash === a.hash
            ? { ...r, change: { ...r.change, label: a.label || null } }
            : r,
        ),
      };
  }
}

/** Whether the value has no known name, only its hash. */
export const unnamed = (c: Change) => c.name.startsWith("#");

/** A hash as eight hex digits. */
export const hex = (hash: number) => hash.toString(16).padStart(8, "0");

/** Items to show. Quiet keeps named and labelled values, `hidden` kinds
 * go, `grep` matches the name, label, hash or type, and notes always show. */
export function visible(
  items: Item[],
  quiet: boolean,
  grep: string,
  hidden: string[] = [],
): Item[] {
  const g = grep.trim().toLowerCase();
  return items.filter((i) => {
    if (!isRow(i)) return true;
    const c = i.change;
    if (quiet && unnamed(c) && !c.label) return false;
    if (hidden.includes(c.kind)) return false;
    if (!g) return true;
    const fields = [c.name, c.label ?? "", hex(c.hash), c.kind];
    return fields.some((f) => f.toLowerCase().includes(g));
  });
}

/** Between a change's old and new value. */
export const ARROW = " -> ";

/** The first change's old value to the latest's new one, or the latest
 * whole when either won't split. */
function span(first: string, latest: string): string {
  const [a, b] = [first.indexOf(ARROW), latest.indexOf(ARROW)];
  return a < 0 || b < 0 ? latest : first.slice(0, a) + latest.slice(b);
}

/** Folds rows next to each other with the same value and source into the
 * latest, counting them and spanning the first old value to the latest. */
export function fold(items: Item[]): Item[] {
  const out: Item[] = [];
  for (const i of items) {
    const last = out.at(-1);
    const same =
      last &&
      isRow(last) &&
      isRow(i) &&
      last.change.hash === i.change.hash &&
      last.change.name === i.change.name &&
      last.save === i.save;
    if (!same) {
      out.push(i);
      continue;
    }
    const change = {
      ...i.change,
      change: span(last.change.change, i.change.change),
    };
    out[out.length - 1] = { ...i, change, count: (last.count ?? 1) + 1 };
  }
  return out;
}

/** An item as one line of text, the form every copy uses. A row is its
 * time, name, kind and change, tab separated, so it pastes as columns. */
export function lineOf(i: Item): string {
  if (!isRow(i)) return `--- ${i.note} ---`;
  const c = i.change;
  const save = i.save ? "save " : "";
  const label = c.label ? ` (${c.label})` : "";
  const times = i.count ? ` ×${i.count}` : "";
  return [
    clock(i.at),
    `${save}${c.name}${label}`,
    c.kind,
    c.change + times,
  ].join("\t");
}

/** Items as lines, one each. */
export const textOf = (items: Item[]) => items.map(lineOf).join("\n");

/** Every kind of value among the rows, and any hidden, sorted. */
export const kindsOf = (items: Item[], hidden: string[]) =>
  [
    ...new Set([...items.filter(isRow).map((r) => r.change.kind), ...hidden]),
  ].sort();

/** `mm:ss` for a time in seconds. */
export function clock(secs: number): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(Math.floor(secs / 60))}:${pad(secs % 60)}`;
}
