import type { Summary } from "@/bindings";
import type { HistoryFilter } from "@/stores/ui";

/** Which seeds each History filter shows. */
export const HISTORY_SHOWS: Record<HistoryFilter, (e: Summary) => boolean> = {
  all: () => true,
  standard: (e) => !e.ap_slot,
  archipelago: (e) => !!e.ap_slot,
  starred: (e) => e.starred,
};
