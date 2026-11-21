import { useEffect } from "react";

/** Runs `open` on ⌘K (Ctrl+K), anywhere in the window. */
export function usePaletteKey(open: () => void) {
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "k") return;
      e.preventDefault();
      open();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [open]);
}
