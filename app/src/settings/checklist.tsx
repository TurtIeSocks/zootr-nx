import { ChevronRight } from "lucide-react";
import { type ReactNode, useId } from "react";
import { ChangedDot } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import { setChecklistOpen, useUi } from "@/stores/ui";

/** One on/off item of a checklist. */
export interface ChecklistItem {
  /** What the change patch keys it by. */
  id: string;
  label: string;
  /** A line under the label. */
  description?: string;
  /** Badges and pips at the trailing edge. */
  tags?: ReactNode;
  on: boolean;
  /** Marks the item with the gold dot: it differs from what drift
   * measures from. */
  changed?: boolean;
  /** Forced on, and why. */
  locked?: boolean;
}

/** Past this many items, a list takes two columns. */
const TWO_COLUMNS = 6;

/** Props for {@link Checklist}. */
export interface ChecklistProps {
  /** Names the card, and keys whether it's open. */
  title: string;
  description?: string;
  /** Lines under the tools, such as why items are locked. */
  note?: ReactNode;
  items: ChecklistItem[];
  /** Takes the new values by item id. */
  onChange: (changes: Record<string, boolean>) => void;
}

/** Many on/off items of one kind in a card that starts closed. The count
 * keeps the state visible while it's closed. */
export function Checklist({
  title,
  description,
  note,
  items,
  onChange,
}: ChecklistProps) {
  const open = useUi((s) => s.settings.open.includes(title));
  const free = items.filter((i) => !i.locked);
  const all = (on: boolean) =>
    onChange(Object.fromEntries(free.map((i) => [i.id, on])));
  const body = useId();
  return (
    <div className="rounded-lg border border-line bg-surface shadow-xs">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={body}
        onClick={() => setChecklistOpen(title, !open)}
        className="group/head flex w-full cursor-pointer items-center gap-2 rounded-lg py-3 pr-3 pl-4 text-left transition-colors duration-(--duration-fast) ease-out hover:bg-surface-hover"
      >
        <ChevronRight className="size-4 shrink-0 text-ink-faint transition-transform duration-(--duration-base) ease-out group-aria-expanded/head:rotate-90" />
        <span className="min-w-0 flex-1">
          <span className="font-medium text-sm">{title}</span>
          {description && (
            <span className="block text-ink-muted text-xs">{description}</span>
          )}
        </span>
        <span className="text-ink-faint text-xs tabular-nums">
          {items.filter((i) => i.on).length} of {items.length} on
        </span>
      </button>
      {/* Grows open rather than popping in, so it stays mounted. */}
      <div
        id={body}
        inert={!open}
        className={cn(
          "grid transition-[grid-template-rows] duration-(--duration-slow) ease-out",
          open ? "grid-rows-[1fr]" : "grid-rows-[0fr]",
        )}
      >
        <div className="min-h-0 overflow-hidden">
          <div className="flex items-center gap-1 pr-4 pb-2 pl-9">
            <Button variant="ghost" size="sm" onClick={() => all(true)}>
              All
            </Button>
            <Button variant="ghost" size="sm" onClick={() => all(false)}>
              None
            </Button>
          </div>
          {note && <div className="pr-4 pb-2 pl-11 empty:hidden">{note}</div>}
          <div
            className={cn(
              "grid gap-x-2 px-2 pb-2",
              items.length >= TWO_COLUMNS && "grid-cols-2",
            )}
          >
            {items.map((i) => (
              <Item
                key={i.id}
                item={i}
                onChange={(on) => onChange({ [i.id]: on })}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

/** Props for {@link Item}. */
interface ItemProps {
  item: ChecklistItem;
  onChange: (on: boolean) => void;
}

/** One item: a checkbox that the whole row toggles. */
function Item({ item, onChange }: ItemProps) {
  const name = useId();
  return (
    // Clicking anywhere on the row toggles the checkbox it wraps.
    // biome-ignore lint/a11y/noLabelWithoutControl: the Radix checkbox inside is a button, which a label can wrap
    <label
      className={cn(
        "flex min-h-9 items-center gap-3 rounded-sm px-2 py-1.5 text-[13px] leading-[18px] transition-colors duration-(--duration-fast) hover:bg-surface-hover",
        item.locked ? "text-ink-muted" : "cursor-pointer",
      )}
    >
      <Checkbox
        aria-labelledby={name}
        checked={item.on}
        disabled={item.locked}
        onCheckedChange={(on) => onChange(on === true)}
      />
      <span className="min-w-0 flex-1">
        <span id={name}>{item.label}</span>
        {item.changed && (
          <span role="img" aria-label="changed" className="ml-1.5">
            <ChangedDot />
          </span>
        )}
        {item.description && (
          <span className="block text-ink-faint text-xs">
            {item.description}
          </span>
        )}
      </span>
      {item.tags && (
        <span className="flex items-center gap-1.5">{item.tags}</span>
      )}
    </label>
  );
}
