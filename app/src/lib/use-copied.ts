import { useEffect, useRef, useState } from "react";

/** How long a button says "Copied", in milliseconds. */
const SHOWN = 1500;

/** What was copied last, for a moment, so its button can say "Copied". The
 * moment ends early on unmount, with nothing left to fire. */
export function useCopied() {
  /** The key of what was copied, until the moment passes. */
  const [copied, setCopied] = useState<string>();
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);

  /** Marks `key` as just copied, restarting the moment. */
  const flash = (key: string) => {
    clearTimeout(timer.current);
    setCopied(key);
    timer.current = setTimeout(() => setCopied(undefined), SHOWN);
  };
  return { copied, flash };
}
