import { type ComponentProps, useLayoutEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

export interface BaseOption {
  id: unknown;
  label: string;
}

interface Props<T extends BaseOption>
  extends Omit<ComponentProps<"div">, "onChange"> {
  options: readonly T[];
  value: T["id"] | undefined;
  onValueChange: (id: T["id"]) => void;
  /** Stretch to the container with equal segments. */
  full?: boolean;
}

/** Two to four short options, the picked one riding a sliding indicator.
 * Each option is a pressed or unpressed button. */
export function Segmented<T extends BaseOption>({
  options,
  value,
  onValueChange,
  full,
  className,
  ...props
}: Props<T>) {
  const track = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{ left: number; width: number }>();

  // Fonts loading or the row reflowing move the segments, so it follows them.
  // biome-ignore lint/correctness/useExhaustiveDependencies: measures again when the pick or the options change
  useLayoutEffect(() => {
    const el = track.current;
    if (!el) return;
    const measure = () => {
      const picked = el.querySelector<HTMLElement>("[aria-pressed=true]");
      setAt(
        picked
          ? { left: picked.offsetLeft, width: picked.offsetWidth }
          : undefined,
      );
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const watch = new ResizeObserver(measure);
    watch.observe(el);
    return () => watch.disconnect();
  }, [value, options]);

  return (
    // biome-ignore lint/a11y/useSemanticElements: a fieldset is for form controls, this groups toggle buttons
    <div
      role="group"
      ref={track}
      className={cn(
        "relative inline-flex rounded-md bg-surface-sunken p-0.5 shadow-[inset_0_0_0_1px_var(--line)]",
        full && "flex",
        className,
      )}
      {...props}
    >
      {at && (
        <span
          aria-hidden
          className="absolute top-0.5 bottom-0.5 left-0 rounded-sm bg-surface-raised shadow-[var(--shadow-xs),0_0_0_1px_var(--line)] transition-[transform,width] duration-(--duration-base) ease-spring"
          style={{ transform: `translateX(${at.left}px)`, width: at.width }}
        />
      )}
      {options.map((o) => (
        <button
          key={`${o.id}`}
          type="button"
          aria-pressed={o.id === value}
          onClick={() => onValueChange(o.id)}
          className={cn(
            "relative z-1 h-6.5 cursor-pointer whitespace-nowrap rounded-sm px-3 font-medium text-[12.5px] text-ink-muted transition-colors duration-(--duration-fast) ease-out hover:text-ink aria-pressed:text-ink",
            full && "flex-1",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
