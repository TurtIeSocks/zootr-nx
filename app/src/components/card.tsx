import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Props for {@link Card}. */
export interface CardProps {
  title?: ReactNode;
  description?: ReactNode;
  /** Quiet text at the header's trailing edge, such as a count. */
  meta?: ReactNode;
  action?: ReactNode;
  className?: string;
  children: ReactNode;
}

/** A `surface` card on the canvas. The header shows when there's a title. */
export function Card({ className, children, ...header }: CardProps) {
  return (
    <section
      className={cn(
        "rounded-lg border border-line bg-surface shadow-xs",
        className,
      )}
    >
      {header.title && <CardHeader {...header} title={header.title} />}
      {children}
    </section>
  );
}

/** Props for {@link CardHeader}. */
export interface CardHeaderProps
  extends Omit<CardProps, "className" | "children" | "title"> {
  title: ReactNode;
}

/** A card's title, description, and trailing meta and action. A folder's
 * sections use it too. */
export function CardHeader({
  title,
  description,
  meta,
  action,
}: CardHeaderProps) {
  return (
    <div className="flex items-start justify-between gap-4 px-4 pt-4 pb-2">
      <div className="min-w-0">
        <h2 className="font-semibold text-[15px] leading-[22px] tracking-[-0.005em]">
          {title}
        </h2>
        {description && (
          <p className="mt-0.5 text-[13px] text-ink-muted leading-[18px]">
            {description}
          </p>
        )}
      </div>
      {(meta || action) && (
        <div className="flex shrink-0 items-center gap-3">
          {meta && (
            <span className="whitespace-nowrap text-ink-faint text-xs tabular-nums">
              {meta}
            </span>
          )}
          {action}
        </div>
      )}
    </div>
  );
}
