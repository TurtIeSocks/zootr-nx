import { type LucideIcon, Search, X } from "lucide-react";
import { Fragment, useEffect, useRef, useState } from "react";
import { Marked } from "@/components/palette/marked";
import { words as wordsOf } from "@/lib/matching";
import { cn } from "@/lib/utils";

/** One result: what it shows. What it does is the caller's. */
export interface PaletteItem {
  /** Unique among the items. */
  id: string;
  /** The id of the group it lists under. */
  group: string;
  icon: LucideIcon;
  title: string;
  /** Where it sits, shown before the title: a docs site's trail. */
  trail?: string[];
  /** One line under the title. */
  body?: string;
  /** Greyed, for one that can't run now. */
  muted?: boolean;
}

/** A group's id and heading. */
export interface PaletteGroup {
  id: string;
  title: string;
}

/** A key the footer explains. */
export interface PaletteKey {
  keys: string[];
  label: string;
}

/** Props for {@link Palette}. */
export interface PaletteProps<T extends PaletteItem> {
  /** Names the dialog and its field. */
  label: string;
  placeholder: string;
  query: string;
  onQuery: (query: string) => void;
  /** The groups, in the order they list. */
  groups: PaletteGroup[];
  /** The results, in each group's order. */
  items: T[];
  /** The most each group shows while others show too. Past it, a line
   * counts the rest. */
  cap?: number;
  /** The one group shown, as a chip before the field. */
  scope?: PaletteGroup;
  /** Sets or clears the scope. Given, a group's "+N more" scopes to it. */
  onScope?: (group: PaletteGroup | undefined) => void;
  /** Said with nothing typed and nothing to show. */
  hint: string;
  onRun: (item: T) => void;
  onClose: () => void;
  /** A key beyond ↑↓, ↵ and esc. True when it took the key. */
  onKey?: (key: string, item: T | undefined) => boolean;
  /** The footer's extra keys, before esc. */
  keys?: PaletteKey[];
  /** What ↵ does, in the footer. */
  runLabel?: string;
  /** Places the scrim: absolute in a panel, fixed over a window. */
  className?: string;
  panelClassName?: string;
}

/** A group's shown items and how many more it has. */
interface Shown<T> {
  group: PaletteGroup;
  items: T[];
  more: number;
}

/** Each group with items, capped while more than one has any. */
function shownBy<T extends PaletteItem>(
  groups: PaletteGroup[],
  items: T[],
  cap: number,
): Shown<T>[] {
  const all = groups
    .map((group) => ({ group, all: items.filter((i) => i.group === group.id) }))
    .filter((g) => g.all.length > 0);
  const most = all.length > 1 ? cap : Number.POSITIVE_INFINITY;
  return all.map(({ group, all }) => ({
    group,
    items: all.slice(0, most),
    more: all.length - most,
  }));
}

/** A search over grouped results: ↑↓ move, ↵ runs the highlighted one,
 * esc or a click on the scrim closes. Each window fills it with its own
 * results. */
export function Palette<T extends PaletteItem>({
  label,
  placeholder,
  query,
  onQuery,
  groups,
  items,
  cap = Number.POSITIVE_INFINITY,
  scope,
  onScope,
  hint,
  onRun,
  onClose,
  onKey,
  keys = [],
  runLabel = "run",
  className,
  panelClassName,
}: PaletteProps<T>) {
  // The highlight starts again with each query or scope.
  const key = `${scope?.id ?? ""}\n${query}`;
  const [sel, setSel] = useState({ key, at: 0 });
  const shown = shownBy(
    scope ? [scope] : groups,
    scope ? items.filter((i) => i.group === scope.id) : items,
    cap,
  );
  const flat = shown.flatMap((s) => s.items);
  const at = Math.min(sel.key === key ? sel.at : 0, flat.length - 1);
  const setAt = (i: number) => setSel({ key, at: i });
  // From the latest highlight, so presses faster than a render all count.
  const step = (by: number) =>
    setSel((s) => {
      const from = s.key === key ? s.at : 0;
      return { key, at: Math.max(0, Math.min(from + by, flat.length - 1)) };
    });
  const hit = flat[at];

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") step(1);
    else if (e.key === "ArrowUp") step(-1);
    else if (e.key === "Enter") hit && onRun(hit);
    else if (e.key === "Escape") onClose();
    else if (e.key === "Backspace" && scope && !query) onScope?.(undefined);
    else if (!onKey?.(e.key, hit)) return;
    e.preventDefault();
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the scrim closes on a click; Esc closes from the keyboard
    <div
      className={cn("z-50 bg-scrim", className)}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        role="dialog"
        aria-modal
        aria-label={label}
        className={cn(
          "overflow-hidden rounded-xl bg-surface-raised shadow-lg",
          panelClassName,
        )}
      >
        <div className="flex items-center gap-2 border-line border-b px-3">
          <Search aria-hidden className="size-4 text-ink-faint" />
          {scope && (
            <span className="flex shrink-0 items-center gap-0.5 rounded bg-primary-soft py-0.5 pr-0.5 pl-1.5 text-[12px] text-primary-ink">
              <span data-scope>{scope.title}</span>
              <button
                type="button"
                aria-label="Search everything"
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => onScope?.(undefined)}
                className="cursor-pointer rounded p-0.5 hover:bg-surface-hover"
              >
                <X aria-hidden className="size-3" />
              </button>
            </span>
          )}
          <input
            // biome-ignore lint/a11y/noAutofocus: the palette opens to type in
            autoFocus
            aria-label={label}
            placeholder={placeholder}
            className="h-11 min-w-0 flex-1 bg-transparent text-[14px] outline-none"
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            onKeyDown={onKeyDown}
          />
        </div>
        {flat.length === 0 ? (
          <p className="px-3 py-3 text-[13px] text-ink-faint">
            {query.trim() ? "No matches" : hint}
          </p>
        ) : (
          <div className="max-h-[60vh] overflow-auto py-1">
            {shown.map((s) => (
              <Group
                key={s.group.id}
                shown={s}
                words={wordsOf(query)}
                active={hit}
                onHover={(item) => setAt(flat.indexOf(item))}
                onRun={onRun}
                onMore={onScope && (() => onScope(s.group))}
              />
            ))}
          </div>
        )}
        <Keys keys={keys} runLabel={runLabel} />
      </div>
    </div>
  );
}

/** Props for {@link Group}. */
interface GroupProps<T> {
  shown: Shown<T>;
  words: string[];
  active: T | undefined;
  onHover: (item: T) => void;
  onRun: (item: T) => void;
  /** Shows the rest: a click on the "+N more" line. */
  onMore?: () => void;
}

/** A group's heading, its items, and how many it leaves out. */
function Group<T extends PaletteItem>({
  shown: { group, items, more },
  words,
  active,
  onHover,
  onRun,
  onMore,
}: GroupProps<T>) {
  return (
    <div>
      <h3 className="px-3 pt-2 pb-1 font-semibold text-[11px] text-ink-faint uppercase tracking-wide">
        {group.title}
      </h3>
      <ul aria-label={group.title}>
        {items.map((item) => (
          <li key={item.id}>
            <Row
              item={item}
              words={words}
              current={item === active}
              onHover={() => onHover(item)}
              onRun={() => onRun(item)}
            />
          </li>
        ))}
      </ul>
      {more > 0 &&
        (onMore ? (
          <button
            type="button"
            tabIndex={-1}
            onMouseDown={(e) => e.preventDefault()}
            onClick={onMore}
            className="mx-1 cursor-pointer rounded px-3 pt-0.5 pb-1 text-[12px] text-ink-faint hover:text-ink"
          >
            +{more} more
          </button>
        ) : (
          <p className="px-4 pt-0.5 pb-1 text-[12px] text-ink-faint">
            +{more} more
          </p>
        ))}
    </div>
  );
}

/** Props for {@link Row}. */
interface RowProps {
  item: PaletteItem;
  words: string[];
  current: boolean;
  onHover: () => void;
  onRun: () => void;
}

/** One result: its trail and title, and its line under them. */
function Row({ item, words, current, onHover, onRun }: RowProps) {
  const { icon: Icon, trail, title, body } = item;
  const ref = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    // The arrows can walk past the list's fold. Optional: jsdom lacks it.
    if (current) ref.current?.scrollIntoView?.({ block: "nearest" });
  }, [current]);
  return (
    <button
      ref={ref}
      type="button"
      tabIndex={-1}
      aria-current={current}
      onMouseDown={(e) => e.preventDefault()}
      onMouseMove={current ? undefined : onHover}
      onClick={onRun}
      className={cn(
        "mx-1 flex w-[calc(100%-0.5rem)] cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-surface-hover",
        current && "bg-primary-soft hover:bg-primary-soft",
        item.muted && "text-ink-muted",
      )}
    >
      <Icon
        aria-hidden
        className={cn(
          "size-4 shrink-0 text-ink-faint",
          current && "text-primary-ink",
        )}
      />
      <span className="flex min-w-0 flex-1 flex-col">
        {trail?.length ? (
          <span className="truncate text-[12px] text-ink-faint">
            {trail.map((t) => (
              <Fragment key={t}>
                <Marked text={t} words={words} />
                {" › "}
              </Fragment>
            ))}
            <Marked
              text={title}
              words={words}
              className={cn(
                "font-medium text-[13px]",
                !item.muted && "text-ink",
              )}
            />
          </span>
        ) : (
          <Marked text={title} words={words} />
        )}
        {body && (
          <Marked
            text={body}
            words={words}
            className="text-[12px] text-ink-muted"
          />
        )}
      </span>
    </button>
  );
}

/** What each key does. */
function Keys({ keys, runLabel }: { keys: PaletteKey[]; runLabel: string }) {
  const kbd =
    "rounded-xs border border-line-strong px-1 font-sans text-[11px] text-ink-muted";
  const all = [
    { keys: ["↑", "↓"], label: "move" },
    { keys: ["↵"], label: runLabel },
    ...keys,
  ];
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-line border-t px-3 py-2 text-[12px] text-ink-faint">
      {all.map((k) => (
        <span key={k.label}>
          {k.keys.map((key) => (
            <Fragment key={key}>
              <kbd className={kbd}>{key}</kbd>{" "}
            </Fragment>
          ))}
          {k.label}
        </span>
      ))}
      <span className="ml-auto">
        <kbd className={kbd}>esc</kbd> close
      </span>
    </div>
  );
}
