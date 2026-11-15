import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Props for {@link Meter}. */
interface MeterProps {
  value: number;
  max: number;
  className?: string;
  children?: ReactNode;
}

/** A thin progress bar with its count beside it. The text carries the
 * meaning, so the bar is hidden from screen readers. */
export function Meter({ value, max, className, children }: MeterProps) {
  const share = max > 0 ? Math.min(1, value / max) : 0;
  return (
    <span
      className={cn(
        "flex items-center gap-2 text-ink-muted text-xs tabular-nums",
        className,
      )}
    >
      <span
        aria-hidden
        className="h-1.5 w-24 shrink-0 overflow-hidden rounded-full bg-surface-sunken ring-1 ring-line-strong ring-inset"
      >
        <span
          className="block h-full rounded-full bg-primary transition-[width] duration-(--duration-base) ease-out"
          style={{ width: `${share * 100}%` }}
        />
      </span>
      {children}
    </span>
  );
}
