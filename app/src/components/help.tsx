import { CircleHelp } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { type HelpTopic, TOPICS, type Topic } from "@/help/topics";
import { go } from "@/stores/shell";

/** How long the pointer rests before the card opens, and how long it
 * lingers once the pointer leaves the icon and the card. */
const OPEN_MS = 200;
const CLOSE_MS = 150;

/** Props for {@link Help}. */
export interface HelpProps {
  topic: HelpTopic;
}

/** A small "?" beside a label that explains it. Resting the pointer opens
 * the card, which stays while the pointer is on it so its link can be
 * clicked. A click or Enter pins it open until Escape, a click outside or
 * a second click. Opening never takes focus. */
export function Help({ topic }: HelpProps) {
  const t: Topic = typeof topic === "string" ? TOPICS[topic] : topic;
  const [open, setOpen] = useState(false);
  const pinned = useRef(false);
  const timer = useRef<number>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const hover = (over: boolean) => {
    window.clearTimeout(timer.current);
    if (pinned.current) return;
    timer.current = window.setTimeout(
      () => setOpen(over),
      over ? OPEN_MS : CLOSE_MS,
    );
  };
  const close = () => {
    window.clearTimeout(timer.current);
    pinned.current = false;
    setOpen(false);
  };
  return (
    <Popover open={open} onOpenChange={(next) => !next && close()}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`About ${t.title}`}
          className="inline-flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-full text-ink-faint transition-colors hover:text-ink-muted focus-visible:text-ink-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          onPointerEnter={() => hover(true)}
          onPointerLeave={() => hover(false)}
          onClick={(e) => {
            e.preventDefault();
            if (pinned.current) return close();
            window.clearTimeout(timer.current);
            pinned.current = true;
            setOpen(true);
          }}
        >
          <CircleHelp aria-hidden className="size-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-72"
        onOpenAutoFocus={(e) => e.preventDefault()}
        onPointerEnter={() => hover(true)}
        onPointerLeave={() => hover(false)}
      >
        <p className="font-medium text-sm">{t.title}</p>
        {t.body.map((line) => (
          <p
            key={line}
            className="mt-1 text-[13px] text-ink-muted leading-[18px]"
          >
            {line}
          </p>
        ))}
        {t.link && (
          <Button
            variant="ghost"
            size="sm"
            className="mt-2 -ml-2"
            onClick={() => {
              close();
              if (t.link) go(t.link.to);
            }}
          >
            {t.link.label}
          </Button>
        )}
      </PopoverContent>
    </Popover>
  );
}
