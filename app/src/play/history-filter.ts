import type { Summary } from "@/bindings";
import { useSeeds } from "@/stores/seeds";
import { useShell } from "@/stores/shell";
import { type HistoryFilter, useUi } from "@/stores/ui";

/** Which seeds each History filter shows. */
export const HISTORY_SHOWS: Record<HistoryFilter, (e: Summary) => boolean> = {
  all: () => true,
  standard: (e) => !e.ap_slot,
  archipelago: (e) => !!e.ap_slot,
  starred: (e) => e.starred,
};

/** History draws this many rows at first, and this many more each time
 * it's asked for more. */
export const PAGE = 100;

/** Draws History far enough down to hold seed `id` under its filter, so
 * Find can reveal a row past the first page. */
export function drawTo(id: number) {
  const shows = HISTORY_SHOWS[useUi.getPath("play.history")];
  const at = useSeeds
    .getState()
    .history.entries.filter(shows)
    .findIndex((e) => e.id === id);
  const need = Math.ceil((at + 1) / PAGE) * PAGE;
  if (need > (useShell.getState().historyDrawn ?? PAGE))
    useShell.setPath("historyDrawn", need);
}
