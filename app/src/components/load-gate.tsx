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

/** The line shown while a first load is in flight. */
export function Loading({ className }: { className?: string }) {
  return (
    <div className={cn("py-12 text-center text-ink-muted", className)}>
      Loading…
    </div>
  );
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
  if (!error) return <Loading className={className} />;
  return (
    <div className={cn("py-12 text-center text-ink-muted", className)}>
      <InlineError error={error} className="mx-auto w-fit text-left" />
    </div>
  );
}
