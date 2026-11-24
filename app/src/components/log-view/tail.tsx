import { ArrowDown } from "lucide-react";
import {
  type ReactNode,
  type RefObject,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import type { Selection } from "@/components/log-view/use-selection";
import { Button } from "@/components/ui/button";

/** Props for {@link Tail}. */
interface TailProps {
  /** The lines shown, by id: ids at or past {@link next} are new. A late
   * line slotted in below it is shown but not counted. */
  shown: { id: number }[];
  /** The next line's id, read when following stops. */
  next: () => number;
  selection: Selection;
  /** The scrolling box, for a caller that measures it. */
  box?: RefObject<HTMLDivElement | null>;
  children: ReactNode;
}

/** A log's scrolling box, following new lines while scrolled to the
 * bottom. Over it, a button counts what arrived since and jumps back down.
 * A press off every line clears the selection. */
export function Tail({
  shown,
  next,
  selection,
  box: given,
  children,
}: TailProps) {
  const own = useRef<HTMLDivElement>(null);
  const box = given ?? own;
  const [follow, setFollow] = useState(true);
  const [seen, setSeen] = useState(0);
  const waiting = follow ? 0 : shown.filter((r) => r.id >= seen).length;

  // biome-ignore lint/correctness/useExhaustiveDependencies: scrolls again as rows arrive
  useLayoutEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [follow, shown.length]);

  const scrolled = () => {
    const el = box.current;
    if (!el) return;
    const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < 8;
    if (bottom === follow) return;
    setFollow(bottom);
    if (!bottom) setSeen(next());
  };

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={box}
        onScroll={scrolled}
        onPointerDown={(e) => {
          if (!(e.target as Element).closest("[data-line], [data-header]"))
            selection.clear();
        }}
        className="h-full overflow-auto pb-1 font-mono text-xs leading-[18px]"
      >
        {children}
      </div>
      {waiting > 0 && (
        <Button
          variant="secondary"
          size="sm"
          className="absolute bottom-3 left-1/2 -translate-x-1/2 shadow-md"
          onClick={() => setFollow(true)}
        >
          {waiting} new
          <ArrowDown />
        </Button>
      )}
    </div>
  );
}
