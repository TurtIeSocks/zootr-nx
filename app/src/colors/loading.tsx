import { Loader2 } from "lucide-react";

/** A spinner over a preview while the game's files are read. */
export function Loading({ label }: { label: string }) {
  return (
    <div
      role="status"
      aria-label={label}
      className="pointer-events-none absolute inset-0 grid place-items-center"
    >
      <Loader2 className="size-6 animate-spin text-ink-muted" />
    </div>
  );
}
