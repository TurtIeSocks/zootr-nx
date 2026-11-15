import { useEffect, useRef, useState } from "react";

/** Pops once when the item is gained or upgraded: `gained` turns on when
 * `tier` rises, and `done`, at the animation's end, turns it off. */
export function useGained(tier: number) {
  const [gained, setGained] = useState(false);
  const last = useRef(tier);
  useEffect(() => {
    if (tier > last.current) setGained(true);
    last.current = tier;
  }, [tier]);
  return { gained, done: () => setGained(false) };
}
