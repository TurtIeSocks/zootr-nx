import { useEffect, useState } from "react";
import { message } from "@/lib/api";

/** A load's data once in, why it failed, or still loading. */
export type Loaded<T> =
  | { status: "loading" }
  | { status: "ok"; data: T }
  | { status: "error"; error: string };

/** Loads with `load(key)`, and again whenever `key` changes. An answer for
 * a key that has since changed is dropped. `load` should be a module-level
 * function, so it never changes. */
export function useLoad<T, K>(load: (key: K) => Promise<T>, key: K): Loaded<T> {
  const [loaded, setLoaded] = useState<Loaded<T>>({ status: "loading" });
  useEffect(() => {
    let live = true;
    setLoaded({ status: "loading" });
    load(key)
      .then((data) => live && setLoaded({ status: "ok", data }))
      .catch((e) => live && setLoaded({ status: "error", error: message(e) }));
    return () => {
      live = false;
    };
  }, [load, key]);
  return loaded;
}
