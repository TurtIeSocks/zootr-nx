import { useEffect } from "react";
import type { Selection } from "@/components/log-view/use-selection";

/** Whether a key press belongs to a field rather than the page. */
export const typing = (e: KeyboardEvent) =>
  e.target instanceof HTMLElement &&
  (e.target.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName));

/** ⌘C copies the selection, ⌘A selects every line shown, Esc clears,
 * outside any field. */
export function useLogKeys(
  shown: { id: number }[],
  selection: Selection,
  copy: () => void,
) {
  const selecting = selection.ids.size > 0;
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (typing(e)) return;
      const cmd = e.metaKey || e.ctrlKey;
      if (e.key === "Escape") selection.clear();
      else if (cmd && e.key === "a") selection.all(shown);
      else if (cmd && e.key === "c" && selecting) copy();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });
}
