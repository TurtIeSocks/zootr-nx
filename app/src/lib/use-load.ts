import { useEffect, useState } from "react";
import { message } from "@/lib/api";

/** A load's data once in, why it failed, or still loading. */
export type Loaded<T> =
  | { status: "loading" }
  | { status: "ok"; data: T }
  | { status: "error"; error: string };

/** Loads with `load(key)`, and again whenever `key` changes. An answer for
 * a key that has since changed is dropped. `load` should be a module-level
 * function, so it never changes. With `keep`, what loaded last stays up
 * while the next load runs, rather than going back to loading. */
export function useLoad<T, K>(
  load: (key: K) => Promise<T>,
  key: K,
  keep = false,
): Loaded<T> {
  const [loaded, setLoaded] = useState<Loaded<T>>({ status: "loading" });
  useEffect(() => {
    let live = true;
    setLoaded((l) => (keep && l.status === "ok" ? l : { status: "loading" }));
    load(key)
      .then((data) => live && setLoaded({ status: "ok", data }))
      .catch((e) => live && setLoaded({ status: "error", error: message(e) }));
    return () => {
      live = false;
    };
  }, [load, key, keep]);
  return loaded;
}
