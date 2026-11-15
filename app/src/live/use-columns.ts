import {
  type CSSProperties,
  createContext,
  type KeyboardEvent,
  type PointerEvent,
  type RefObject,
  useContext,
  useEffect,
  useState,
} from "react";
import { COLUMNS, useUi, type Widths } from "@/stores/ui";

/** A resizable column of the log. */
export type Column = keyof Widths;

/** The log's grid. Widths come from `--live-cols`, set by
 * {@link useColumnsStyle}. */
export const ROW_GRID = "grid grid-cols-[var(--live-cols)] gap-3 px-4";

/** The narrowest each column goes. */
const MIN: Widths = { time: 40, name: 100, type: 40 };
/** The narrowest the Change column goes. The others leave room for it. */
const CHANGE_MIN = 100;
/** A row's padding, `px-4`, and the `gap-3` between its four columns. */
const CHROME = 2 * 16 + 3 * 12;
/** The order columns give way in a narrow log: Name, which grows longest,
 * then Type, then Time. */
const GIVE: Column[] = ["name", "type", "time"];
/** The columns left to right. */
const ORDER: Column[] = ["time", "name", "type"];
/** How far an arrow key moves a divider. */
const STEP = 16;

const total = (w: Widths) => w.time + w.name + w.type;

/** The widths drawn in a log `room` pixels wide: as saved when they fit
 * with Change at its minimum, else shrunk in {@link GIVE} order, none below
 * its own minimum. A log not yet measured, `room` 0, changes nothing. */
export function fitTo(w: Widths, room: number): Widths {
  if (room <= 0) return w;
  let over = total(w) + CHROME + CHANGE_MIN - room;
  const out = { ...w };
  for (const col of GIVE) {
    if (over <= 0) break;
    const give = Math.min(over, out[col] - MIN[col]);
    out[col] -= give;
    over -= give;
  }
  return out;
}

/** The saved widths after `col` is resized to `px`. Growing past the room
 * takes from the columns to its right, nearest first, down to their
 * minimums, then stops where Change keeps its minimum. */
export function resize(
  col: Column,
  px: number,
  saved: Widths,
  room: number,
): Widths {
  const out = { ...saved, [col]: Math.max(MIN[col], Math.round(px)) };
  if (room <= 0) return out;
  const drawn = { ...fitTo(saved, room), [col]: out[col] };
  let over = total(drawn) + CHROME + CHANGE_MIN - room;
  for (const c of ORDER.slice(ORDER.indexOf(col) + 1)) {
    const give = Math.min(over, drawn[c] - MIN[c]);
    if (give <= 0) continue;
    out[c] = drawn[c] - give;
    over -= give;
  }
  if (over > 0) out[col] = Math.max(MIN[col], out[col] - over);
  return out;
}

/** The log's inner width, for {@link fitTo}. 0 until it's measured. */
export const Room = createContext(0);

/** The width of `box`, kept current as it resizes. */
export function useRoom(box: RefObject<HTMLElement | null>): number {
  const [room, setRoom] = useState(0);
  useEffect(() => {
    const el = box.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const measure = () => setRoom(el.clientWidth);
    measure();
    const watch = new ResizeObserver(measure);
    watch.observe(el);
    return () => watch.disconnect();
  }, [box]);
  return room;
}

/** The widths drawn: the saved ones, fit to the room. */
function useDrawn(): Widths {
  const [saved] = useUi.usePath("live.columns");
  return fitTo(saved, useContext(Room));
}

/** The `--live-cols` style for the widths drawn, for the box around the
 * rows. */
export function useColumnsStyle(): CSSProperties {
  const w = useDrawn();
  return {
    "--live-cols": `${w.time}px ${w.name}px ${w.type}px minmax(${CHANGE_MIN}px,1fr)`,
  } as CSSProperties;
}

/** Props for the divider on `col`'s right edge: drag or arrow keys
 * resize, double-click resets, all through {@link resize}. Widths are kept in the ui store. */
export function useDivider(col: Column) {
  const room = useContext(Room);
  const drawn = useDrawn();
  /** Resizes from `from`, the widths a drag started with, so dragging back
   * gives the neighbours back what they gave. Else from the current ones. */
  const set = (px: number, from?: Widths) =>
    useUi.setPath("live.columns", (w) => resize(col, px, from ?? w, room));
  return {
    role: "separator",
    tabIndex: 0,
    "aria-orientation": "vertical" as const,
    "aria-label": `Resize ${col}`,
    "aria-valuenow": drawn[col],
    onPointerDown: (e: PointerEvent) => {
      e.preventDefault();
      const [x, start] = [e.clientX, drawn[col]];
      const from = useUi.getPath("live.columns");
      const move = (m: globalThis.PointerEvent) =>
        set(start + m.clientX - x, from);
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    },
    onKeyDown: (e: KeyboardEvent) => {
      const by = { ArrowLeft: -STEP, ArrowRight: STEP }[e.key];
      if (by) set(drawn[col] + by);
    },
    onDoubleClick: () => set(COLUMNS[col]),
  };
}
