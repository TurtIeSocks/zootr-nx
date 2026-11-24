import { ChevronRight } from "lucide-react";
import { type ReactNode, useId, useState } from "react";
import { cn } from "@/lib/utils";

/** Props for {@link Disclosure}. */
export interface DisclosureProps {
  title: ReactNode;
  description?: string;
  /** Quiet text at the header's trailing edge, such as a count. */
  meta?: ReactNode;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Leaves the body out until it first opens. */
  lazy?: boolean;
  children: ReactNode;
}

/** A card that opens and closes from its header. */
export function Disclosure({
  title,
  description,
  meta,
  open,
  onOpenChange,
  lazy = false,
  children,
}: DisclosureProps) {
  const body = useId();
  const [opened, setOpened] = useState(open);
  if (open && !opened) setOpened(true);
  return (
    <section className="rounded-lg border border-line bg-surface shadow-xs">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={body}
        onClick={() => onOpenChange(!open)}
        className="group/head flex w-full cursor-pointer items-center gap-2 rounded-lg py-3 pr-3 pl-4 text-left transition-colors duration-(--duration-fast) ease-out hover:bg-surface-hover"
      >
        <ChevronRight className="size-4 shrink-0 text-ink-faint transition-transform duration-(--duration-base) ease-out group-aria-expanded/head:rotate-90" />
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2 font-medium text-sm">
            {title}
          </span>
          {description && (
            <span className="block text-ink-muted text-xs">{description}</span>
          )}
        </span>
        {meta && (
          <span className="text-ink-faint text-xs tabular-nums">{meta}</span>
        )}
      </button>
      {/* Grows open rather than popping in, so it stays mounted. */}
      <div
        id={body}
        inert={!open}
        className={cn(
          "grid transition-[grid-template-rows] duration-(--duration-slow) ease-out",
          open ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
        )}
      >
        <div className="min-h-0 overflow-hidden">
          {(opened || !lazy) && children}
        </div>
      </div>
    </section>
  );
}
