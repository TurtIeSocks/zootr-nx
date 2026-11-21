import {
  ChevronUp,
  CircleAlert,
  Eye,
  FlaskConical,
  Gift,
  LoaderCircle,
  type LucideIcon,
  Navigation,
  Play,
  RefreshCw,
  Signpost,
  ToggleRight,
  Variable,
} from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { since, useNow } from "@/dev/time";
import { cn } from "@/lib/utils";
import { type Entry, type Kind, useDev } from "@/stores/dev";

/** Each kind of request's icon. */
const ICONS: Record<Kind, LucideIcon> = {
  warp: Navigation,
  hide: Eye,
  age: RefreshCw,
  switch: ToggleRight,
  give: Gift,
  step: Signpost,
  set: Variable,
  apply: FlaskConical,
  play: Play,
};

/** An entry's icon: its kind's, or an alert when it failed. */
function EntryIcon({ entry }: { entry: Entry }) {
  const Icon = entry.ok ? ICONS[entry.kind] : CircleAlert;
  return <Icon aria-hidden className="size-4 shrink-0" />;
}

/** The line at the window's foot: what's running, else the newest result
 * and how long ago, with the last results a click away. */
export function Activity() {
  const [log] = useDev.usePath("activity.log");
  const [running] = useDev.usePath("activity.running");
  const [open, setOpen] = useState(false);
  const last = log[0];
  const failed = !running && last && !last.ok;

  return (
    <footer className="relative shrink-0 border-line border-t bg-surface">
      {open && <History log={log} />}
      <div
        className={cn(
          "flex h-8.5 items-center gap-2 pr-1.5 pl-3 text-[13px]",
          failed && "bg-danger-soft text-danger-ink",
        )}
      >
        {running ? (
          <>
            <LoaderCircle
              aria-hidden
              className="size-4 shrink-0 animate-spin text-ink-muted"
            />
            <span className="min-w-0 flex-1 truncate first-letter:uppercase">
              {running.what}…
            </span>
          </>
        ) : last ? (
          <>
            <EntryIcon entry={last} />
            <span
              role={last.ok ? "status" : "alert"}
              className="min-w-0 flex-1 truncate"
            >
              {last.text}
            </span>
            <Ago at={last.at} />
          </>
        ) : (
          <span className="flex-1 text-ink-faint">
            Results of what you send show here
          </span>
        )}
        <Button
          size="icon-xs"
          variant="ghost"
          aria-label={open ? "Hide the last results" : "Show the last results"}
          aria-expanded={open}
          disabled={log.length === 0}
          onClick={() => setOpen(!open)}
        >
          <ChevronUp className={cn(open && "rotate-180")} />
        </Button>
      </div>
    </footer>
  );
}

/** The last results, newest first, each line of one on its own line. */
function History({ log }: { log: Entry[] }) {
  return (
    <ul
      aria-label="Last results"
      className="absolute inset-x-0 bottom-full max-h-80 overflow-auto border-line border-t bg-surface-raised py-1 shadow-md"
    >
      {log.map((e) => (
        <li
          key={`${e.at}-${e.text}`}
          className={cn(
            "flex items-start gap-2 px-3 py-1 text-[12.5px]",
            !e.ok && "text-danger-ink",
          )}
        >
          <EntryIcon entry={e} />
          <span className="min-w-0 flex-1 whitespace-pre-line break-words">
            {e.text}
          </span>
          <Ago at={e.at} />
        </li>
      ))}
    </ul>
  );
}

/** How long ago `at` was, ticking. */
export function Ago({ at }: { at: number }) {
  const now = useNow();
  return (
    <span className="shrink-0 text-ink-faint tabular-nums">
      {since(at, Math.max(now, at))}
    </span>
  );
}
