import { type DevStars, useUi } from "@/stores/ui";

/** `list` with `key` added at the end, or taken out when it's there. */
export const toggled = (list: string[], key: string) =>
  list.includes(key) ? list.filter((k) => k !== key) : [...list, key];

/** One kind of star: what's starred, and a toggle for one key. */
export function useStars(kind: keyof DevStars) {
  const [list, setList] = useUi.usePath(`dev.stars.${kind}`);
  return {
    list,
    has: (key: string) => list.includes(key),
    toggle: (key: string) => setList((l) => toggled(l, key)),
  };
}
