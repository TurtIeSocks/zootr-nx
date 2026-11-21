import {
  ArrowDown,
  Check,
  Copy,
  Flag,
  FolderOpen,
  ListCollapse,
  Search,
} from "lucide-react";
import {
  type FormEvent,
  type PointerEvent,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { commands } from "@/bindings";
import { ArmedButton } from "@/components/armed-button";
import { InlineError } from "@/components/callout";
import { Help } from "@/components/help";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Segmented } from "@/components/ui/segmented";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { call } from "@/lib/api";
import { MOD } from "@/lib/platform";
import { useRequest, useRequestState } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { useCopied } from "@/lib/use-copied";
import { LiveHeader, LiveRow, NoteRow } from "@/live/live-row";
import { fold, type Item, isRow, kindsOf, textOf, visible } from "@/live/rows";
import { Room, useColumnsStyle, useRoom } from "@/live/use-columns";
import { useFresh } from "@/live/use-fresh";
import { type Selection, useSelection } from "@/live/use-selection";
import { Page } from "@/shell/page-header";
import { LiveDot } from "@/shell/status-pill";
import { clear, mark, useLive } from "@/stores/live";
import { useUi } from "@/stores/ui";

/** Muted values listed in the tooltip before it gives a count. */
const LISTED = 20;

/** What the watcher is doing. */
const WATCHING = {
  Waiting: "Waiting for the game",
  Attaching: "Connecting to the game",
  Recording: "Recording",
};

/** Quiet hides unnamed values; All shows them. */
const VIEWS = [
  { id: true, label: "Quiet" },
  { id: false, label: "All" },
];

/** Says what the watcher is doing, when the session falls back to autosaves, and
 * why. */
function SourceNote() {
  const [st] = useLive.usePath("status");
  const [s] = useLive.usePath("session");
  const [unsaved] = useLive.usePath("unsaved");
  const on = st.state === "Recording";
  const reason = "reason" in st ? st.reason : on && s?.reason;
  return (
    <div className="flex min-w-0 items-center gap-2">
      <LiveDot state={st.state} />
      <span className="font-medium text-sm">{WATCHING[st.state]}</span>
      <Help topic="live-status" />
      {on && s?.source === "Saves" && <Badge variant="info">Autosaves</Badge>}
      {on && unsaved !== "" && (
        <Tip focusable tip={unsaved}>
          <Badge variant="danger">Not saved to a file</Badge>
        </Tip>
      )}
      {reason && (
        <span className="truncate text-[13px] text-ink-faint">{reason}</span>
      )}
    </div>
  );
}

/** How many values are muted this session, listing them on hover. */
function MutedCount() {
  const [muted] = useLive.usePath("muted");
  if (muted.length === 0) return null;
  const more = muted.length - LISTED;
  return (
    <Tooltip>
      <TooltipTrigger asChild tabIndex={0}>
        <span className="cursor-default whitespace-nowrap text-ink-faint text-xs">
          {muted.length} muted
        </span>
      </TooltipTrigger>
      <TooltipContent className="font-mono">
        {muted.slice(0, LISTED).map((m) => (
          <div key={m.hash}>{m.name}</div>
        ))}
        {more > 0 && <div>and {more} more</div>}
      </TooltipContent>
    </Tooltip>
  );
}

/** What the log shows with nothing in it. */
function Empty() {
  const [state] = useLive.usePath("status.state");
  return (
    <p className="px-4 py-12 text-center font-sans text-ink-muted text-sm">
      {state === "Recording"
        ? "Waiting for changes"
        : "Recording starts when the game does"}
    </p>
  );
}

/** The log's view: Quiet or All. */
function ViewPicker() {
  const [quiet, setQuiet] = useUi.usePath("live.quiet");
  return <Segmented options={VIEWS} value={quiet} onValueChange={setQuiet} />;
}

/** Filters the log by name or change. */
function GrepInput() {
  const [grep, setGrep] = useUi.usePath("live.grep");
  return (
    <div className="relative w-44">
      <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-ink-faint" />
      <Input
        aria-label="Filter lines"
        placeholder="Filter lines"
        className="h-7 pl-8"
        value={grep}
        onChange={(e) => setGrep(e.target.value)}
      />
    </div>
  );
}

/** The lines shown, after the view and the filter, folded if folding. */
function useShown() {
  const [items] = useLive.usePath("items");
  const [quiet] = useUi.usePath("live.quiet");
  const [grep] = useUi.usePath("live.grep");
  const [folding] = useUi.usePath("live.fold");
  const [hidden] = useUi.usePath("live.hiddenKinds");
  const shown = visible(items, quiet, grep, hidden);
  return folding ? fold(shown) : shown;
}

/** Picks which kinds of value the log shows. */
function TypesMenu() {
  const [items] = useLive.usePath("items");
  const [hidden, setHidden] = useUi.usePath("live.hiddenKinds");
  const kinds = kindsOf(items, hidden);
  const show = (kind: string, on: boolean) =>
    setHidden(on ? hidden.filter((k) => k !== kind) : [...hidden, kind]);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="ghost" disabled={kinds.length === 0}>
          Types
          {hidden.length > 0 && (
            <span className="text-ink-faint">· {hidden.length} hidden</span>
          )}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="max-h-80 overflow-y-auto">
        <DropdownMenuLabel>Show these types</DropdownMenuLabel>
        {kinds.map((kind) => (
          <DropdownMenuCheckboxItem
            key={kind}
            checked={!hidden.includes(kind)}
            onCheckedChange={(on) => show(kind, on)}
            onSelect={(e) => e.preventDefault()}
            className="font-mono text-xs"
          >
            {kind}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Whether a key press belongs to a field rather than the page. */
const typing = (e: KeyboardEvent) =>
  e.target instanceof HTMLElement &&
  (e.target.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName));

/** Marks the log with what happened, from a popover that M also opens.
 * Only while recording, since the session writes it. */
function MarkButton() {
  const [state] = useLive.usePath("status.state");
  const [open, setOpen] = useState(false);
  const on = state === "Recording";
  useEffect(() => {
    if (!on) return;
    const key = (e: KeyboardEvent) => {
      if (e.key !== "m" || e.metaKey || e.ctrlKey || e.altKey || typing(e))
        return;
      e.preventDefault();
      setOpen(true);
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [on]);
  return (
    <Popover open={open && on} onOpenChange={setOpen}>
      <Tip
        wrap
        tip={
          on
            ? "Drops a note in the log at this moment (M)"
            : "Marks need a recording"
        }
      >
        <PopoverTrigger asChild>
          <Button size="sm" variant="ghost" disabled={!on}>
            <Flag />
            Mark
          </Button>
        </PopoverTrigger>
      </Tip>
      <PopoverContent align="end" className="w-72 p-2">
        <MarkForm onDone={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}

/** Props for {@link MarkForm}. */
interface MarkFormProps {
  onDone: () => void;
}

/** The marker's text. Enter adds it; blank marks it "Mark". It mounts with
 * the popover, so each opening starts empty. */
function MarkForm({ onDone }: MarkFormProps) {
  const [what, setWhat] = useState("");
  const save = useRequestState();
  const submit = (e: FormEvent) => {
    e.preventDefault();
    save.run(async () => {
      await mark(what);
      onDone();
    });
  };
  return (
    <form onSubmit={submit}>
      <Input
        autoFocus
        aria-label="What happened?"
        placeholder="What happened?"
        className="h-7"
        value={what}
        onChange={(e) => setWhat(e.target.value)}
      />
      <InlineError error={save.error} className="mt-1" />
    </form>
  );
}

/** Folds back to back changes of one value, or stops. */
function FoldButton() {
  const [folding, setFolding] = useUi.usePath("live.fold");
  return (
    <TipButton
      label="Fold repeats"
      tip={
        folding
          ? "Repeats of one value fold into one row"
          : "Fold repeats of one value into one row"
      }
      variant={folding ? "secondary" : "ghost"}
      pressed={folding}
      onClick={() => setFolding(!folding)}
    >
      <ListCollapse />
    </TipButton>
  );
}

/** Props for {@link CopyButton}. */
interface CopyButtonProps {
  /** The selected lines, or every line shown with none selected. */
  lines: Item[];
  selecting: boolean;
  copied: boolean;
  onCopy: () => void;
}

/** Copies the selected lines, or every line shown, one per line. */
function CopyButton({ lines, selecting, copied, onCopy }: CopyButtonProps) {
  const n = lines.length;
  const label = selecting
    ? `Copy ${n} ${n === 1 ? "row" : "rows"}`
    : "Copy shown";
  const what = selecting ? "the selected rows" : "every row shown";
  return (
    <TipButton
      label={label}
      tip={copied ? "Copied" : `Copy ${what}, one line each (${MOD}C)`}
      variant="ghost"
      disabled={n === 0}
      onClick={onCopy}
    >
      {copied ? <Check /> : <Copy />}
    </TipButton>
  );
}

/** Shows the session log in its folder, through the page's request. */
function OpenLogButton() {
  const [log] = useLive.usePath("session.log");
  const { run } = useRequest();
  return (
    <TipButton
      label="Open log"
      tip="Show the session log in its folder"
      variant="ghost"
      disabled={!log}
      onClick={() => log && run(() => call(commands.revealLog(log)))}
    >
      <FolderOpen />
    </TipButton>
  );
}

/** Why the page's request or the live store last failed. */
function LiveError() {
  const { error } = useRequest();
  const [failed] = useLive.usePath("error");
  return <InlineError error={error ?? failed} className="px-4 pt-2" />;
}

/** How many older rows went to the session log only. */
function Dropped() {
  const [dropped] = useLive.usePath("dropped");
  if (dropped === 0) return null;
  return (
    <p className="px-4 py-1 font-sans text-ink-faint">
      {dropped} older rows are only in the session log.
    </p>
  );
}

/** Props for {@link ColumnsBox}. */
interface ColumnsBoxProps {
  children: ReactNode;
}

/** Sets the column widths for the rows inside, fit to the log's width. A
 * divider drag redraws only this box, not the rows, whose elements stay
 * the same. */
function ColumnsBox({ children }: ColumnsBoxProps) {
  return (
    // Only a log too narrow for every column's minimum scrolls sideways.
    <div className="min-w-fit" style={useColumnsStyle()}>
      {children}
    </div>
  );
}

/** The watcher's log, live. */
export function LiveView() {
  const shown = useShown();
  const selection = useSelection();
  const { copied, flash } = useCopied();
  const selecting = selection.ids.size > 0;
  const lines = selecting
    ? shown.filter((i) => selection.ids.has(i.id))
    : shown;
  const copy = async () => {
    await navigator.clipboard.writeText(textOf(lines));
    flash("lines");
  };

  // ⌘C copies, ⌘A selects every line, Esc clears, outside any field.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (typing(e)) return;
      const cmd = e.metaKey || e.ctrlKey;
      if (e.key === "Escape") selection.clear();
      else if (cmd && e.key === "a") selection.all(shown);
      else if (cmd && e.key === "c" && selecting) copy();
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  return (
    <Page wide className="min-h-0 flex-1">
      <RequestProvider>
        <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-line border-b px-3 py-2">
            <SourceNote />
            <div className="ml-auto flex items-center gap-2">
              <ViewPicker />
              <GrepInput />
              <TypesMenu />
              <FoldButton />
              <MutedCount />
              <MarkButton />
              <Tip tip="Clear every line">
                <ArmedButton
                  size="sm"
                  verb="clear"
                  onAct={() => {
                    clear();
                    selection.clear();
                  }}
                >
                  Clear
                </ArmedButton>
              </Tip>
              <CopyButton
                lines={lines}
                selecting={selecting}
                copied={copied !== undefined}
                onCopy={copy}
              />
              <OpenLogButton />
            </div>
          </div>
          <LiveError />
          <Log shown={shown} selection={selection} />
        </section>
      </RequestProvider>
    </Page>
  );
}

/** Props for {@link Log}. */
interface LogProps {
  shown: Item[];
  selection: Selection;
}

/** The rows shown, following new ones while scrolled to the bottom. Above
 * it, a button counts what arrived since and jumps back down. A press off
 * every line clears the selection. */
function Log({ shown, selection }: LogProps) {
  const [log] = useLive.usePath("session.log");
  const [follow, setFollow] = useState(true);
  const [seen, setSeen] = useState(0);
  const fresh = useFresh(log ?? null);
  const box = useRef<HTMLDivElement>(null);
  const room = useRoom(box);
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
    if (!bottom) setSeen(useLive.getPath("next"));
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
        <Room.Provider value={room}>
          <ColumnsBox>
            <LiveHeader />
            <Dropped />
            {shown.map((i) => {
              const line = {
                selected: selection.ids.has(i.id),
                pointer: {
                  onPointerDown: (e: PointerEvent) =>
                    selection.press(e, i.id, shown),
                  onPointerEnter: () => selection.enter(i.id, shown),
                },
              };
              return isRow(i) ? (
                <LiveRow key={i.id} row={i} fresh={fresh(i.id)} {...line} />
              ) : (
                <NoteRow key={i.id} note={i} {...line} />
              );
            })}
            {shown.length === 0 && <Empty />}
          </ColumnsBox>
        </Room.Provider>
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
