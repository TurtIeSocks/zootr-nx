import { useEffect, useState } from "react";
import { message } from "@/lib/api";

/** A load's data once in, why it failed, or still loading. */
export type Loaded<T> =
  | { status: "loading" }
  | { status: "ok"; data: T }
  | { status: "error"; error: string };

/** The state of a load not in yet, one object so nothing redraws for it. */
const LOADING = { status: "loading" } as const;

/** Loads with `load(key)`, and again whenever `key` changes. An answer for
 * a key that has since changed is dropped. `load` should be a module-level
 * function, so it never changes. With `keep`, what loaded last stays up
 * while the next load runs, rather than going back to loading. */
export function useLoad<T, K>(
  load: (key: K) => Promise<T>,
  key: K,
  keep = false,
): Loaded<T> {
  // Each answer carries its key, so a render for a new key never draws the
  // old key's answer before the effect catches up.
  const [loaded, setLoaded] = useState<{ key: K; state: Loaded<T> }>();
  useEffect(() => {
    let live = true;
    const answer = (state: Loaded<T>) => live && setLoaded({ key, state });
    load(key)
      .then((data) => answer({ status: "ok", data }))
      .catch((e) => answer({ status: "error", error: message(e) }));
    return () => {
      live = false;
    };
  }, [load, key]);
  if (loaded && Object.is(loaded.key, key)) return loaded.state;
  return keep && loaded?.state.status === "ok" ? loaded.state : LOADING;
}
