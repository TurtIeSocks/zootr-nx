import { type LucideIcon, Search } from "lucide-react";
import { Fragment, useState } from "react";
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
  /** The most each group shows. Past it, a line counts the rest. */
  cap?: number;
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

/** Each group with items, capped. */
function shownBy<T extends PaletteItem>(
  groups: PaletteGroup[],
  items: T[],
  cap: number,
): Shown<T>[] {
  return groups.flatMap((group) => {
    const all = items.filter((i) => i.group === group.id);
    if (all.length === 0) return [];
    return [{ group, items: all.slice(0, cap), more: all.length - cap }];
  });
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
  hint,
  onRun,
  onClose,
  onKey,
  keys = [],
  runLabel = "run",
  className,
  panelClassName,
}: PaletteProps<T>) {
  // The highlight starts again with each query.
  const [sel, setSel] = useState({ query, at: 0 });
  const shown = shownBy(groups, items, cap);
  const flat = shown.flatMap((s) => s.items);
  const at = Math.min(sel.query === query ? sel.at : 0, flat.length - 1);
  const setAt = (i: number) => setSel({ query, at: i });
  const hit = flat[at];

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") setAt(Math.min(at + 1, flat.length - 1));
    else if (e.key === "ArrowUp") setAt(Math.max(at - 1, 0));
    else if (e.key === "Enter") hit && onRun(hit);
    else if (e.key === "Escape") onClose();
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
}

/** A group's heading, its items, and how many it leaves out. */
function Group<T extends PaletteItem>({
  shown: { group, items, more },
  words,
  active,
  onHover,
  onRun,
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
      {more > 0 && (
        <p className="px-4 pt-0.5 pb-1 text-[12px] text-ink-faint">
          +{more} more
        </p>
      )}
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
  return (
    <button
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
