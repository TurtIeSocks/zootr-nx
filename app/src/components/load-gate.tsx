import type { ReactNode } from "react";
import { InlineError } from "@/components/callout";
import { cn } from "@/lib/utils";

/** Props for {@link LoadGate}. */
export interface LoadGateProps {
  /** Whether the store's first load is in. */
  loaded: boolean;
  /** Why the load failed, shown in place of "Loading…". */
  error?: string;
  className?: string;
  /** What shows once loaded. */
  children: ReactNode;
}

/** A store's children once its first load is in; "Loading…", or why it
 * failed, until then. */
export function LoadGate({
  loaded,
  error,
  className,
  children,
}: LoadGateProps) {
  if (loaded) return children;
  return (
    <div className={cn("py-12 text-center text-ink-muted", className)}>
      {error ? (
        <InlineError error={error} className="mx-auto w-fit text-left" />
      ) : (
        "Loading…"
      )}
    </div>
  );
}
