import {
  ArrowDownLeft,
  ArrowUpRight,
  Check,
  CircleCheck,
  Copy,
  Flag,
  type LucideIcon,
  Plug,
  Skull,
  TriangleAlert,
  WifiOff,
} from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { InlineError } from "@/components/callout";
import { CardHeader } from "@/components/card";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { message } from "@/lib/api";
import { useCopied } from "@/lib/use-copied";
import { cn } from "@/lib/utils";
import {
  keeps,
  kindOf,
  type LineKind,
  type LogFilter,
  SENT,
} from "@/play/ap-lines";
import { type ApLine, useAp } from "@/stores/ap";

const FILTERS = [
  { id: "all", label: "All" },
  { id: "items", label: "Items" },
  { id: "room", label: "Room" },
] as const satisfies readonly { id: LogFilter; label: string }[];

/** Each kind's icon and colour. */
const LOOK: Record<LineKind, { icon: LucideIcon; tone: string }> = {
  sent: { icon: ArrowUpRight, tone: "" },
  received: { icon: ArrowDownLeft, tone: "[&_svg]:text-primary-ink" },
  death: { icon: Skull, tone: "text-danger-ink [&_svg]:text-danger-ink" },
  goal: {
    icon: Flag,
    tone: "font-semibold text-primary-ink [&_svg]:text-primary-ink",
  },
  lost: { icon: WifiOff, tone: "text-gold-ink [&_svg]:text-gold-ink" },
  refused: { icon: TriangleAlert, tone: "text-gold-ink [&_svg]:text-gold-ink" },
  room: { icon: Plug, tone: "text-ink-muted" },
  trouble: {
    icon: TriangleAlert,
    tone: "text-gold-ink [&_svg]:text-gold-ink",
  },
};

/** A note's time, as the log shows it. */
const clock = (time: number) =>
  new Date(time).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });

/** How close to the end counts as at the newest line, in pixels. */
const NEAR_END = 8;

/** What the Archipelago client did, newest last: the folder's last
 * section. It keeps to the newest line until scrolled up. */
export function ApLog() {
  const lines = useAp((s) => s.lines);
  const [filter, setFilter] = useState<LogFilter>("all");
  const { copied, flash } = useCopied();
  const [error, setError] = useState<string>();
  const shown = lines.filter((l) => keeps(filter, kindOf(l.text)));
  const list = useRef<HTMLOListElement>(null);
  const [following, setFollowing] = useState(true);

  // biome-ignore lint/correctness/useExhaustiveDependencies: follows each new line
  useLayoutEffect(() => {
    const el = list.current;
    if (following && el) el.scrollTop = el.scrollHeight;
  }, [shown.length, following]);

  const copy = async () => {
    const text = lines.map((l) => `${clock(l.time)} ${l.text}`).join("\n");
    try {
      await navigator.clipboard.writeText(text);
      setError(undefined);
      flash("log");
    } catch (e) {
      setError(message(e));
    }
  };

  return (
    <section className="border-line border-t">
      <CardHeader
        title="Client Log"
        action={
          <div className="flex items-center gap-2">
            <Segmented
              aria-label="Show"
              options={FILTERS}
              value={filter}
              onValueChange={setFilter}
            />
            <TipButton
              label="Copy log"
              tip={copied ? "Copied" : "Copy the whole log"}
              variant="ghost"
              size="icon-sm"
              disabled={!lines.length}
              onClick={copy}
            >
              {copied ? <Check /> : <Copy />}
            </TipButton>
          </div>
        }
      />
      <InlineError className="px-4 pb-2" error={error} />
      {shown.length ? (
        <div className="relative">
          <ol
            ref={list}
            aria-label="Client Log"
            className="max-h-[248px] overflow-auto pt-1 pb-2 font-mono text-[12px] leading-[18px]"
            onScroll={(e) => {
              const el = e.currentTarget;
              setFollowing(
                el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_END,
              );
            }}
          >
            {shown.map((line, i) => (
              // Rows hold no state, so an index keys them, though the log
              // drops its oldest past its kept lines and a filter moves them.
              // biome-ignore lint/suspicious/noArrayIndexKey: stateless rows
              <Row key={i} line={line} />
            ))}
          </ol>
          {!following && (
            <Button
              size="sm"
              variant="secondary"
              className="-translate-x-1/2 absolute bottom-2 left-1/2"
              onClick={() => setFollowing(true)}
            >
              Jump to Latest
            </Button>
          )}
        </div>
      ) : (
        <p className="px-4 pb-4 text-[13px] text-ink-muted">
          {lines.length
            ? "Nothing of this kind yet."
            : "Nothing yet. Connect to a room to see what the client sends and gets."}
        </p>
      )}
    </section>
  );
}

/** One note: its time, its kind's icon and its words. */
function Row({ line }: { line: ApLine }) {
  const kind = kindOf(line.text);
  const { icon, tone } = LOOK[kind];
  const Icon =
    kind === "room" && line.text === "connected" ? CircleCheck : icon;
  return (
    <li
      data-kind={kind}
      className={cn(
        "grid grid-cols-[64px_16px_minmax(0,1fr)] items-center gap-2 px-4 py-[3px] hover:bg-surface-hover [&_svg]:size-[13px] [&_svg]:text-ink-faint",
        tone,
      )}
    >
      <span className="text-ink-faint tabular-nums">{clock(line.time)}</span>
      <Icon />
      <Tip tip={line.text}>
        <span className="truncate">
          <Words kind={kind} text={line.text} />
        </span>
      </Tip>
    </li>
  );
}

/** A note's words, its items picked out. */
function Words({ kind, text }: { kind: LineKind; text: string }) {
  const got = kind === "received" && /^received (.+) from (.+)$/.exec(text);
  if (got) {
    return (
      <>
        received <b className="font-medium text-primary-ink">{got[1]}</b>{" "}
        <span className="text-ink-muted">from {got[2]}</span>
      </>
    );
  }
  if (kind === "sent") {
    return (
      <>
        {SENT}
        <b className="font-medium text-ink">{text.slice(SENT.length)}</b>
      </>
    );
  }
  return text;
}
