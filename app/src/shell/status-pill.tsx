import type { LiveStatus } from "@/bindings";
import { cn } from "@/lib/utils";

/** Props for {@link Ping}. */
interface PingProps {
  className: string;
}

/** A pulsing ring behind a dot, for things that are live. */
function Ping({ className }: PingProps) {
  return (
    <span
      aria-hidden
      className={cn("absolute inset-0 rounded-full opacity-35", className)}
    />
  );
}

/** Props for {@link StatusPill}. */
interface StatusPillProps {
  found: boolean;
  running?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}

/** Whether the game dump or a target is ready. The label always says
 * missing or running, so the dot never carries it alone. */
export function StatusPill({
  found,
  running = false,
  onClick,
  children,
}: StatusPillProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "inline-flex h-6 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-full border border-line bg-surface px-2 font-medium text-ink text-xs transition-colors duration-(--duration-fast) ease-out hover:bg-surface-hover",
        !found &&
          "border-transparent bg-danger-soft text-danger-ink hover:bg-danger-soft",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "relative size-[7px] shrink-0 rounded-full",
          !found
            ? "bg-danger"
            : running
              ? "bg-primary"
              : "shadow-[inset_0_0_0_1.5px_var(--primary)]",
        )}
      >
        {found && running && (
          <Ping className="-inset-[3px] animate-ping-soft bg-primary" />
        )}
      </span>
      {children}
    </button>
  );
}

/** Props for {@link LiveDot}. */
interface LiveDotProps {
  state: LiveStatus["state"];
}

/** The Live item's dot: pulsing red while recording, a hollow ring while it
 * waits for the game. */
export function LiveDot({ state }: LiveDotProps) {
  if (state !== "Recording")
    return (
      <span
        className="size-2 shrink-0 rounded-full shadow-[inset_0_0_0_1.5px_var(--ink-faint)]"
        role="img"
        aria-label="waiting"
      />
    );
  return (
    <span
      className="relative size-2 shrink-0 rounded-full bg-danger"
      role="img"
      aria-label="recording"
    >
      <Ping className="animate-ping-rec bg-danger opacity-100" />
    </span>
  );
}
