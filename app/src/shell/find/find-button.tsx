import { Search } from "lucide-react";
import { MAC } from "@/lib/platform";
import { useShell } from "@/stores/shell";

/** The shortcut, as this platform writes it. */
const KEYS = MAC ? "⌘K" : "Ctrl K";

/** The top bar's search field, which opens the search. Not part of the
 * drag band, so a click opens it rather than moving the window. */
export function FindButton() {
  return (
    <button
      type="button"
      aria-label="Search"
      onClick={() => useShell.setPath("find", true)}
      className="flex h-7 w-full max-w-[360px] cursor-pointer items-center gap-2 rounded-md border border-line bg-surface px-2.5 text-[13px] text-ink-faint shadow-xs transition-colors duration-(--duration-fast) ease-out hover:border-line-strong hover:text-ink-muted"
    >
      <Search aria-hidden className="size-3.5" />
      <span className="flex-1 text-left">Search…</span>
      <kbd className="rounded-xs border border-line-strong px-1 font-sans text-[11px] text-ink-muted">
        {KEYS}
      </kbd>
    </button>
  );
}
