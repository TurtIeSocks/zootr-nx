import type { Level, Line } from "@/bindings";

/** Lines kept on screen. The app's log file keeps everything. */
export const CAP = 2000;

/** The app log store: its lines, and where they're written. */
export interface AppLog {
  /** The newest {@link CAP} lines, oldest first, each with its `seq` as
   * its id for the selection. */
  lines: (Line & { id: number })[];
  /** The next line's number. Anything below it is already here. */
  next: number;
  /** This run's file. */
  file: string | null;
  /** Why there's no file. */
  unsaved: string | null;
  /** Why picking the log back up failed. */
  error?: string;
}

/** The store before the backend says anything. */
export const EMPTY: AppLog = { lines: [], next: 0, file: null, unsaved: null };

/** `log` with `lines` added, skipping any it already holds. */
export function add(log: AppLog, lines: Line[]): AppLog {
  const fresh = lines.filter((l) => l.seq >= log.next);
  if (fresh.length === 0) return log;
  const all = [...log.lines, ...fresh.map((l) => ({ ...l, id: l.seq }))];
  return {
    ...log,
    lines: all.slice(-CAP),
    next: (fresh.at(-1)?.seq ?? log.next - 1) + 1,
  };
}

/** Every level, worst first. */
export const LEVELS: Level[] = ["error", "warn", "info", "debug"];

/** The lines left after the filter and the hidden levels. Debug lines
 * show only with developer tools on. */
export function visible<T extends Line>(
  lines: T[],
  grep: string,
  hidden: Level[],
  devTools: boolean,
): T[] {
  const needle = grep.trim().toLowerCase();
  return lines.filter(
    (l) =>
      !hidden.includes(l.level) &&
      (devTools || l.level !== "debug") &&
      (needle === "" || l.text.toLowerCase().includes(needle)),
  );
}

/** One line as the file writes it. */
export const lineOf = (l: Line) => `${l.time}  ${l.level.padEnd(5)}  ${l.text}`;

/** Lines as text to paste, one each. */
export const textOf = (lines: Line[]) => lines.map(lineOf).join("\n");
