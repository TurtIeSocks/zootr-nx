import { useEffect, useState } from "react";

/** How long before `now` a time in ms was: "now", "12 s ago", "3 min
 * ago", "2 h ago". */
export function since(at: number, now: number): string {
  const s = Math.floor((now - at) / 1000);
  if (s < 1) return "now";
  if (s < 60) return `${s} s ago`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m} min ago` : `${Math.floor(m / 60)} h ago`;
}

/** The time now, ticking every second while the caller shows. */
export function useNow(): number {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);
  return now;
}
