import { ChevronLeft, ChevronRight } from "lucide-react";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { cn } from "@/lib/utils";

/** A kind of element worth pointing at when it's scrolled out of view. */
export interface EdgeMark {
  selector: string;
  /** Read after "Scroll left, " when one is past that edge. */
  label: string;
  /** The dot's colour. */
  className: string;
}

/** What's past one edge: nothing to scroll to, or the marks out there. */
type Edge = null | EdgeMark[];

/** Props for {@link ScrollEdges}. */
interface ScrollEdgesProps {
  /** Elements that put a dot on the edge they're past. */
  marks?: EdgeMark[];
  className?: string;
  children: ReactNode;
}

/** How far one press scrolls, as a share of the visible width. */
const STEP = 0.8;

/** A scroller that says when it scrolls sideways: a fade and a button on
 * each edge with more past it, and a dot on the button for each mark out
 * there. It never scrolls by itself. */
export function ScrollEdges({
  marks = [],
  className,
  children,
}: ScrollEdgesProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState<{ left: Edge; right: Edge }>({
    left: null,
    right: null,
  });
  const measure = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const box = el.getBoundingClientRect();
    const past = (side: "left" | "right") =>
      marks.filter((m) =>
        [...el.querySelectorAll(m.selector)].some((e) => {
          const r = e.getBoundingClientRect();
          const middle = (r.left + r.right) / 2;
          return side === "left" ? middle < box.left : middle > box.right;
        }),
      );
    const left = el.scrollLeft > 1 ? past("left") : null;
    const right =
      el.scrollLeft + el.clientWidth < el.scrollWidth - 1
        ? past("right")
        : null;
    setEdges((was) => {
      const next = { left, right };
      return JSON.stringify(next) === JSON.stringify(was) ? was : next;
    });
  }, [marks]);
  // Content, glows and size all move what's past an edge.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    measure();
    const changes = new MutationObserver(measure);
    changes.observe(el, { childList: true, subtree: true, attributes: true });
    const sizes =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(measure);
    sizes?.observe(el);
    return () => {
      changes.disconnect();
      sizes?.disconnect();
    };
  }, [measure]);
  const scroll = (by: number) => {
    const el = ref.current;
    el?.scrollBy({ left: by * el.clientWidth * STEP, behavior: "smooth" });
  };
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={ref}
        onScroll={measure}
        className={cn("flex min-h-0 flex-1 flex-col overflow-auto", className)}
      >
        {children}
      </div>
      {edges.left && (
        <EdgeButton side="left" marks={edges.left} onClick={() => scroll(-1)} />
      )}
      {edges.right && (
        <EdgeButton
          side="right"
          marks={edges.right}
          onClick={() => scroll(1)}
        />
      )}
    </div>
  );
}

/** Props for {@link EdgeButton}. */
interface EdgeButtonProps {
  side: "left" | "right";
  marks: EdgeMark[];
  onClick: () => void;
}

/** One edge's fade, with a button that scrolls that way and dots for
 * what's out there. */
function EdgeButton({ side, marks, onClick }: EdgeButtonProps) {
  const Icon = side === "left" ? ChevronLeft : ChevronRight;
  const label = [`Scroll ${side}`, ...marks.map((m) => `${m.label} past here`)];
  return (
    <div
      className={cn(
        "pointer-events-none absolute inset-y-0 flex w-28 items-center",
        side === "left"
          ? "left-0 justify-start bg-linear-to-r pl-2"
          : "right-0 justify-end bg-linear-to-l pr-2",
        "from-surface from-30% via-surface/70 to-transparent",
      )}
    >
      <button
        type="button"
        aria-label={label.join(", ")}
        title={label.join(", ")}
        onClick={onClick}
        className="pointer-events-auto relative flex size-8 cursor-pointer items-center justify-center rounded-full border border-line bg-surface-raised text-ink-muted shadow-md transition-colors duration-(--duration-fast) hover:text-ink [&>svg]:size-4"
      >
        <Icon />
        {marks.length > 0 && (
          <span className="absolute -top-0.5 -right-0.5 flex gap-0.5">
            {marks.map((m) => (
              <i
                key={m.label}
                className={cn(
                  "size-2 rounded-full ring-2 ring-surface",
                  m.className,
                )}
              />
            ))}
          </span>
        )}
      </button>
    </div>
  );
}
