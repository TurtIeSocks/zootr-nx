import type { ReactNode } from "react";
import { ChangedDot } from "@/components/ui/badge";

/** Props for {@link FloatingBar}. */
interface FloatingBarProps {
  /** What's pending, beside the dot. */
  message: ReactNode;
  /** The actions on the right. */
  children: ReactNode;
}

/** Floats at the foot of a page while something on it is pending, so its
 * actions stay in reach however far the page scrolls. */
export function FloatingBar({ message, children }: FloatingBarProps) {
  return (
    <div className="sticky bottom-5 z-10 mx-auto flex w-full max-w-160 animate-rise items-center gap-3 rounded-xl border border-line bg-surface-raised py-2 pr-2 pl-4 text-[13px] shadow-lg">
      <ChangedDot className="size-2" />
      <div className="min-w-0 flex-1">{message}</div>
      {children}
    </div>
  );
}
