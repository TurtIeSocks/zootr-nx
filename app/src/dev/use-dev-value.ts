import { useEffect, useState } from "react";
import { commands, type DevValue } from "@/bindings";
import { call, message } from "@/lib/api";

/** A read of `name`: what the game holds, or why it couldn't say. */
export interface ValueRead {
  name: string;
  value?: DevValue;
  error?: string;
}

/** Reads what the game holds for `name`, again when it changes or `round`
 * moves on. An answer for a name since left is dropped, and only `name`'s
 * read shows, while a reread keeps the last one up. */
export function useDevValue(
  name: string,
  round: number,
): ValueRead | undefined {
  const [read, setRead] = useState<ValueRead>();

  useEffect(() => {
    // Read again after `round` moves on.
    void round;
    if (!name) return;
    let live = true;
    call(commands.devValue(name))
      .then((value) => live && setRead({ name, value }))
      .catch((e) => live && setRead({ name, error: message(e) }));
    return () => {
      live = false;
    };
  }, [name, round]);

  return read?.name === name ? read : undefined;
}
