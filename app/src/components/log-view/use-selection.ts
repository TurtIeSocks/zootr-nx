import { type PointerEvent, useEffect, useRef, useState } from "react";
import type { LineProps } from "@/components/log-view/line";

/** A line a log can pick: anything with an id. */
interface Line {
  id: number;
}

/** The lines picked, by id, and the one a range starts from. */
interface Picked {
  ids: Set<number>;
  anchor?: number;
}

const NONE: Picked = { ids: new Set() };

/** The ids of every line from `from` to `to` in `items`, either way round.
 * Just `to` when `from` is gone. */
export function between(items: Line[], from: number, to: number): Set<number> {
  const a = items.findIndex((i) => i.id === from);
  const b = items.findIndex((i) => i.id === to);
  if (a < 0 || b < 0) return new Set([to]);
  const [lo, hi] = a < b ? [a, b] : [b, a];
  return new Set(items.slice(lo, hi + 1).map((i) => i.id));
}

/** The next pick after a press on `id`: shift spans from the anchor, ⌘ or
 * Ctrl toggles, and a plain press picks it alone, or clears it when it was
 * the only one. */
export function pressed(
  p: Picked,
  id: number,
  items: Line[],
  keys: { shift: boolean; toggle: boolean },
): Picked {
  if (keys.shift && p.anchor !== undefined)
    return { ...p, ids: between(items, p.anchor, id) };
  if (keys.toggle) {
    const ids = new Set(p.ids);
    if (!ids.delete(id)) ids.add(id);
    return { ids, anchor: id };
  }
  if (p.ids.size === 1 && p.ids.has(id)) return NONE;
  return { ids: new Set([id]), anchor: id };
}

/** The log's selection: lines picked by click, shift-click, ⌘-click or a
 * drag. A drag ends on any pointer up. */
export function useSelection() {
  const [picked, setPicked] = useState<Picked>(NONE);
  const dragging = useRef(false);
  useEffect(() => {
    const up = () => {
      dragging.current = false;
    };
    window.addEventListener("pointerup", up);
    return () => window.removeEventListener("pointerup", up);
  }, []);

  /** A press on line `id`. Presses on a row's buttons or fields pass. */
  const press = (e: PointerEvent, id: number, items: Line[]) => {
    if (e.button !== 0 || (e.target as Element).closest("button, input"))
      return;
    const keys = { shift: e.shiftKey, toggle: e.metaKey || e.ctrlKey };
    dragging.current = !keys.shift && !keys.toggle;
    setPicked((p) => pressed(p, id, items, keys));
  };

  /** The pointer reaching line `id`, which a drag spans to. */
  const enter = (id: number, items: Line[]) => {
    if (!dragging.current) return;
    setPicked((p) =>
      p.anchor === undefined ? p : { ...p, ids: between(items, p.anchor, id) },
    );
  };

  return {
    ids: picked.ids,
    press,
    enter,
    /** What line `id` carries: its fill and its pointer handlers. */
    line: (id: number, items: Line[]): LineProps => ({
      selected: picked.ids.has(id),
      pointer: {
        onPointerDown: (e) => press(e, id, items),
        onPointerEnter: () => enter(id, items),
      },
    }),
    clear: () => setPicked(NONE),
    all: (items: Line[]) =>
      setPicked({ ids: new Set(items.map((i) => i.id)), anchor: items[0]?.id }),
  };
}

/** What {@link useSelection} gives. */
export type Selection = ReturnType<typeof useSelection>;
