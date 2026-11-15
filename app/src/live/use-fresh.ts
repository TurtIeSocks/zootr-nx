import { useEffect, useState } from "react";
import type { Live } from "@/live/rows";
import { useLive } from "@/stores/live";
import { useUi } from "@/stores/ui";

/** Marks everything up to `next` as looked at. */
const look = ({ session, next }: Live) =>
  useUi.setPath("live.seen", { log: session?.log ?? null, id: next });

/** Whether a row is new since you last looked. Everything up to now counts
 * as looked at while the page is open. */
export function useFresh(log: string | null) {
  const [last] = useState(() => useUi.getPath("live.seen"));
  useEffect(() => {
    look(useLive.getState());
    return useLive.subscribePath("next", () => look(useLive.getState()));
  }, []);
  return (id: number) => log !== last.log || id >= last.id;
}
