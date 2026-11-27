import { createContext, use } from "react";
import type { Spoiler } from "@/bindings";
import { hasLogic, type SpoilerTab } from "@/play/spoiler/rows";
import { useUi } from "@/stores/ui";

/** The loaded spoiler and the check picked in it. */
export interface SpoilerState {
  spoiler: Spoiler;
  /** The picked check's hash, whose detail line shows. */
  picked?: string;
  /** Picks a check by hash, or none. */
  pick: (hash?: string) => void;
}

/** Provided by the spoiler sheet's gate once the spoiler loads. */
export const SpoilerContext = createContext<SpoilerState | null>(null);

/** The spoiler sheet's loaded spoiler and pick. */
export function useSpoiler(): SpoilerState {
  const state = use(SpoilerContext);
  if (!state) throw new Error("useSpoiler has to be used within <SpoilerGate>");
  return state;
}

/** The tab showing: the one chosen, or All checks for a seed without logic,
 * which has no rounds to show, or Entrances chosen on a seed without them. */
export function useShownTab(): SpoilerTab {
  const { spoiler } = useSpoiler();
  const [tab] = useUi.usePath("spoiler.tab");
  const shown =
    (tab !== "entrances" || spoiler.entrances.length > 0) &&
    (tab !== "key-rings" || spoiler.key_rings.length > 0);
  return hasLogic(spoiler) && shown ? tab : "checks";
}
