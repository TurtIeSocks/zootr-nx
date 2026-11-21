import { ChevronRight } from "lucide-react";
import { type ReactNode, useId } from "react";
import { cn } from "@/lib/utils";
import { setSectionOpen, useUi } from "@/stores/ui";

/** Props for {@link Fold}. */
export interface FoldProps {
  /** Keys whether it's open, across restarts. */
  id: string;
  title: string;
  description?: string;
  /** Quiet text at the header's trailing edge, such as a count. */
  meta?: ReactNode;
  children: ReactNode;
}

/** A settings page's section: a card that opens and closes from its
 * header. Each starts open, and stays as it was left. */
export function Fold({ id, title, description, meta, children }: FoldProps) {
  const open = useUi((s) => !s.settings.closed.includes(id));
  const body = useId();
  return (
    <section className="rounded-lg border border-line bg-surface shadow-xs">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={body}
        onClick={() => setSectionOpen(id, !open)}
        className="group/head flex w-full cursor-pointer items-center gap-2 rounded-lg py-3 pr-3 pl-4 text-left transition-colors duration-(--duration-fast) ease-out hover:bg-surface-hover"
      >
        <ChevronRight className="size-4 shrink-0 text-ink-faint transition-transform duration-(--duration-base) ease-out group-aria-expanded/head:rotate-90" />
        <span className="min-w-0 flex-1">
          <span className="font-medium text-sm">{title}</span>
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
        <div className="min-h-0 overflow-hidden">{children}</div>
      </div>
    </section>
  );
}
