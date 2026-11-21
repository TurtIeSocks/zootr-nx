import { ArrowRight, Check, Copy, VolumeX } from "lucide-react";
import {
  type HTMLAttributes,
  type KeyboardEvent,
  type ReactNode,
  useState,
} from "react";
import type { Change } from "@/bindings";
import { InlineError } from "@/components/callout";
import { Tip } from "@/components/tip";
import { Input } from "@/components/ui/input";
import { useRequest, useRequestState } from "@/lib/request";
import { useCopied } from "@/lib/use-copied";
import { cn } from "@/lib/utils";
import {
  ARROW,
  clock,
  lineOf,
  type Note,
  type Row,
  unnamed,
} from "@/live/rows";
import { type Column, ROW_GRID, useDivider } from "@/live/use-columns";
import { label, mute, useLive } from "@/stores/live";

/** What a line does with the pointer, for the log's selection. */
export type LinePointer = Pick<
  HTMLAttributes<HTMLElement>,
  "onPointerDown" | "onPointerEnter"
>;

/** The pointer handlers and fill a line carries. Each line is
 * `data-line`, so a press off every line clears the selection. */
interface LineProps {
  /** Picked in the log's selection. */
  selected: boolean;
  pointer: LinePointer;
}

/** A selected line's fill, over its hover. */
const PICKED =
  "data-selected:bg-primary-soft data-selected:hover:bg-primary-soft";

/** The log's headings, with the column each resizes. */
const HEADINGS: [string, Column?][] = [
  ["Time", "time"],
  ["Name", "name"],
  ["Type", "type"],
  ["Change"],
];

/** The log's sticky heading row. */
export function LiveHeader() {
  return (
    <div
      data-header
      className={cn(
        ROW_GRID,
        "sticky top-0 z-10 border-line border-b bg-surface py-1.5 font-medium font-sans text-ink-faint",
      )}
    >
      {HEADINGS.map(([text, col]) => (
        <span key={text} className="relative min-w-0">
          <span className="block truncate">{text}</span>
          {col && <ColumnDivider col={col} />}
        </span>
      ))}
    </div>
  );
}

/** Props for {@link ColumnDivider}. */
interface ColumnDividerProps {
  col: Column;
}

/** The handle on a heading's right edge that resizes its column. */
function ColumnDivider({ col }: ColumnDividerProps) {
  return (
    <span
      {...useDivider(col)}
      className="absolute inset-y-0 -right-3 w-3 cursor-col-resize touch-none outline-none after:absolute after:inset-y-0 after:left-1/2 after:w-px after:-translate-x-1/2 after:bg-line-strong hover:after:w-0.5 hover:after:bg-primary focus-visible:after:w-0.5 focus-visible:after:bg-primary"
    />
  );
}

/** Each kind of note's ink. */
const TONES: Record<Note["tone"], string> = {
  start: "text-primary-ink",
  end: "text-ink-faint",
  mark: "text-gold-ink",
};

/** Props for {@link NoteRow}. */
interface NoteRowProps extends LineProps {
  note: Note;
}

/** Where a session starts or ends, or a marker, centred between
 * hairlines. */
export function NoteRow({ note, selected, pointer }: NoteRowProps) {
  return (
    <p
      data-line
      data-selected={selected || undefined}
      {...pointer}
      className={cn(
        PICKED,
        "flex items-center gap-3 px-4 pt-3 pb-1.5 font-medium font-sans before:h-px before:flex-1 before:bg-line after:h-px after:flex-1 after:bg-line",
        TONES[note.tone],
      )}
    >
      {note.note}
    </p>
  );
}

/** Props for {@link LabelInput}. */
interface LabelInputProps {
  /** The value it labels. */
  change: Change;
  /** Closes the editor. */
  onDone: () => void;
}

/** An inline label editor. Enter saves, Esc cancels, empty removes. */
function LabelInput({ change, onDone }: LabelInputProps) {
  const [text, setText] = useState(change.label ?? "");
  const save = useRequestState();
  const key = (e: KeyboardEvent) => {
    if (e.key === "Escape") onDone();
    if (e.key !== "Enter") return;
    save.run(async () => {
      await label(change.hash, text);
      onDone();
    });
  };
  return (
    <div>
      <Input
        autoFocus
        aria-label="Label"
        className="h-6 px-1.5 font-mono text-xs"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={key}
        onBlur={save.error ? undefined : onDone}
      />
      <InlineError error={save.error} className="mt-0.5 font-sans" />
    </div>
  );
}

/** Props for {@link ChangeText}. */
interface ChangeTextProps {
  change: string;
  /** How many changes folded into it, when more than one. */
  count?: number;
}

/** "old -> new" as old, an arrow and new. Anything else as it came. */
function ChangeText({ change, count }: ChangeTextProps) {
  const at = change.indexOf(ARROW);
  return (
    <span className="break-all">
      {at < 0 ? (
        change
      ) : (
        <>
          <span className="text-ink-muted">{change.slice(0, at)}</span>
          <ArrowRight
            aria-label="to"
            className="mx-1.5 inline size-3 stroke-2 align-[-1px] text-ink-faint"
          />
          <span className="font-medium text-primary-ink">
            {change.slice(at + ARROW.length)}
          </span>
        </>
      )}
      {count && (
        <Tip focusable tip={`${count} changes in a row`}>
          <span className="ml-2 rounded-xs bg-surface-sunken px-1 text-ink-muted">
            ×{count}
          </span>
        </Tip>
      )}
    </span>
  );
}

/** Props for {@link LiveRow}. */
interface LiveRowProps extends LineProps {
  row: Row;
  /** New since you last looked, so it flashes once. */
  fresh: boolean;
}

/** One change: when, what, its kind and the change itself. An unnamed
 * value's name opens a label editor. */
export function LiveRow({ row, fresh, selected, pointer }: LiveRowProps) {
  const c = row.change;
  const [editing, setEditing] = useState(false);
  return (
    <div
      data-line
      data-selected={selected || undefined}
      {...pointer}
      className={cn(
        ROW_GRID,
        PICKED,
        "group/row relative py-0.75 transition-colors duration-(--duration-fast) hover:bg-surface-hover",
        fresh && "animate-flash",
      )}
    >
      <span className="text-ink-faint tabular-nums">{clock(row.at)}</span>
      {editing ? (
        <LabelInput change={c} onDone={() => setEditing(false)} />
      ) : unnamed(c) ? (
        <Tip tip="Rename this value">
          <button
            type="button"
            className="cursor-pointer truncate text-left text-ink-muted hover:underline"
            onClick={() => setEditing(true)}
          >
            <RowName row={row} />
          </button>
        </Tip>
      ) : (
        <span className="truncate">
          <RowName row={row} />
        </span>
      )}
      <span className="text-ink-faint">{c.kind}</span>
      <ChangeText change={c.change} count={row.count} />
      <RowActions row={row} />
    </div>
  );
}

/** Props for {@link RowActions}. */
interface RowActionsProps {
  row: Row;
}

/** A row's buttons, over the trailing edge while it's hovered or focused.
 * Mute only while recording, since it asks the session. */
function RowActions({ row }: RowActionsProps) {
  const { copied, flash } = useCopied();
  const { run } = useRequest();
  const [state] = useLive.usePath("status.state");
  const copy = async () => {
    await navigator.clipboard.writeText(lineOf(row));
    flash("row");
  };
  return (
    <span className="absolute inset-y-0 right-2 flex items-center gap-0.5 bg-surface-hover pl-2 group-data-selected/row:bg-primary-soft opacity-0 transition-opacity duration-(--duration-fast) focus-within:opacity-100 group-hover/row:opacity-100">
      <RowButton label={copied ? "Copied" : "Copy row"} onClick={copy}>
        {copied ? <Check /> : <Copy />}
      </RowButton>
      {state === "Recording" && (
        <RowButton
          label="Mute this value"
          onClick={() => run(() => mute(row.change.hash))}
        >
          <VolumeX />
        </RowButton>
      )}
    </span>
  );
}

/** Props for {@link RowButton}. */
interface RowButtonProps {
  label: string;
  onClick: () => void;
  children: ReactNode;
}

/** A small icon button among a row's actions. */
function RowButton({ label, onClick, children }: RowButtonProps) {
  return (
    <Tip tip={label}>
      <button
        type="button"
        aria-label={label}
        onClick={onClick}
        className="grid size-5 cursor-pointer place-items-center rounded-xs text-ink-faint outline-none hover:bg-surface hover:text-ink focus-visible:ring-2 focus-visible:ring-focus [&_svg]:size-3.5"
      >
        {children}
      </button>
    </Tip>
  );
}

/** Props for {@link RowName}. */
interface RowNameProps {
  row: Row;
}

/** The value's name, marked when it came from a save, then its label. */
function RowName({ row }: RowNameProps) {
  const c = row.change;
  return (
    <>
      {row.save && (
        <span className="mr-1.5 rounded-xs bg-surface-sunken px-1 text-ink-muted">
          Save
        </span>
      )}
      {c.name}
      {c.label && <span className="ml-2 text-ink-muted">{c.label}</span>}
    </>
  );
}
