import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Props for {@link DevCard}. */
interface DevCardProps {
  title?: string;
  icon?: LucideIcon;
  /** Quiet text at the header's trailing edge. */
  meta?: ReactNode;
  className?: string;
  children: ReactNode;
}

/** A card of a dev tab: an icon and title over its controls. */
export function DevCard({
  title,
  icon: Icon,
  meta,
  className,
  children,
}: DevCardProps) {
  return (
    <section
      aria-label={title}
      className={cn(
        "rounded-lg border border-line bg-surface p-3 shadow-xs",
        className,
      )}
    >
      {title && (
        <div className="mb-2.5 flex items-center gap-2">
          {Icon && <Icon aria-hidden className="size-4 text-ink-muted" />}
          <h2 className="font-semibold text-[13.5px]">{title}</h2>
          {meta && (
            <span className="ml-auto truncate text-ink-faint text-xs">
              {meta}
            </span>
          )}
        </div>
      )}
      {children}
    </section>
  );
}

/** A small uppercase heading over a group of rows, with a count. */
export function GroupHead({
  title,
  count,
  sticky,
}: {
  title: string;
  count?: ReactNode;
  sticky?: boolean;
}) {
  return (
    <h3
      className={cn(
        "flex justify-between bg-surface px-3 pt-2 pb-1 font-semibold text-[11px] text-ink-faint uppercase tracking-wide",
        sticky && "sticky top-0 z-1",
      )}
    >
      <span className="truncate">{title}</span>
      {count !== undefined && <span className="tabular-nums">{count}</span>}
    </h3>
  );
}
