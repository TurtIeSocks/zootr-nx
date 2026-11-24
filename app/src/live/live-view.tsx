import { Flag, ListCollapse } from "lucide-react";
import {
  type FormEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { ArmedButton } from "@/components/armed-button";
import { InlineError } from "@/components/callout";
import { Help } from "@/components/help";
import { HiddenMenu } from "@/components/log-view/hidden-menu";
import { typing } from "@/components/log-view/keys";
import {
  CopyButton,
  GrepInput,
  OpenLogButton,
} from "@/components/log-view/log-tools";
import { Tail } from "@/components/log-view/tail";
import { useLogCopy } from "@/components/log-view/use-log-copy";
import {
  type Selection,
  useSelection,
} from "@/components/log-view/use-selection";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
import { useRequest, useRequestState } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { LiveHeader, LiveRow, NoteRow } from "@/live/live-row";
import { fold, type Item, isRow, kindsOf, textOf, visible } from "@/live/rows";
import { Room, useColumnsStyle, useRoom } from "@/live/use-columns";
import { useFresh } from "@/live/use-fresh";
import { Page } from "@/shell/page-header";
import { LiveDot } from "@/shell/status-pill";
import { clear, mark, useLive } from "@/stores/live";
import { useUi } from "@/stores/ui";

/** Muted values listed in the tooltip before it gives a count. */
const MUTED_LISTED = 20;

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
          <Badge variant="danger">Not Saved to a File</Badge>
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
  const more = muted.length - MUTED_LISTED;
  return (
    <Tooltip>
      <TooltipTrigger asChild tabIndex={0}>
        <span className="cursor-default whitespace-nowrap text-ink-faint text-xs">
          {muted.length} muted
        </span>
      </TooltipTrigger>
      <TooltipContent className="font-mono">
        {muted.slice(0, MUTED_LISTED).map((m) => (
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

/** Filters the log by name or change. */
function LiveGrep() {
  const [grep, setGrep] = useUi.usePath("live.grep");
  return <GrepInput value={grep} onChange={setGrep} />;
}

/** The log's view: Quiet or All. */
function ViewPicker() {
  const [quiet, setQuiet] = useUi.usePath("live.quiet");
  return <Segmented options={VIEWS} value={quiet} onValueChange={setQuiet} />;
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
  return (
    <HiddenMenu
      label="Types"
      heading="Show These Types"
      options={kinds}
      hidden={hidden}
      onHiddenChange={setHidden}
      disabled={kinds.length === 0}
      className="max-h-80 overflow-y-auto"
      itemClassName="font-mono text-xs"
    />
  );
}

/** Marks the log with what happened, from a popover that M also opens.
 * Only while recording, since the session writes it. */
function MarkButton() {
  const [state] = useLive.usePath("status.state");
  const [open, setOpen] = useState(false);
  const on = state === "Recording";
  useEffect(() => {
    // A popover left open when recording stops stays shut on the next one.
    if (!on) {
      setOpen(false);
      return;
    }
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

/** Shows the session log in its folder. */
function LiveLogButton() {
  const [log] = useLive.usePath("session.log");
  return <OpenLogButton log={log} what="session log" />;
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
  const copyButton = useLogCopy(shown, selection, textOf);

  return (
    <Page wide className="min-h-0 flex-1">
      <RequestProvider>
        <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-line border-b px-3 py-2">
            <SourceNote />
            <div className="ml-auto flex items-center gap-2">
              <ViewPicker />
              <LiveGrep />
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
              <CopyButton {...copyButton} />
              <LiveLogButton />
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

/** The rows shown, in a box that follows new ones. */
function Log({ shown, selection }: LogProps) {
  const [log] = useLive.usePath("session.log");
  const fresh = useFresh(log ?? null);
  const box = useRef<HTMLDivElement>(null);
  const room = useRoom(box);
  return (
    <Tail
      shown={shown}
      next={() => useLive.getPath("next")}
      selection={selection}
      box={box}
    >
      <Room.Provider value={room}>
        <ColumnsBox>
          <LiveHeader />
          <Dropped />
          {shown.map((i) => {
            const line = selection.line(i.id, shown);
            return isRow(i) ? (
              <LiveRow key={i.id} row={i} fresh={fresh(i.id)} {...line} />
            ) : (
              <NoteRow key={i.id} note={i} {...line} />
            );
          })}
          {shown.length === 0 && <Empty />}
        </ColumnsBox>
      </Room.Provider>
    </Tail>
  );
}
