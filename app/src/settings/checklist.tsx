import { type ReactNode, useId } from "react";
import { ChangedDot } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";
import { Fold } from "@/settings/fold";

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
  /** Keys whether it's open. */
  id: string;
  title: string;
  description?: string;
  /** Lines under the tools, such as why items are locked. */
  note?: ReactNode;
  items: ChecklistItem[];
  /** Takes the new values by item id. */
  onChange: (changes: Record<string, boolean>) => void;
}

/** Many on/off items of one kind in a section of their own. The count
 * keeps the state visible while it's closed. */
export function Checklist({
  id,
  title,
  description,
  note,
  items,
  onChange,
}: ChecklistProps) {
  const free = items.filter((i) => !i.locked);
  const all = (on: boolean) =>
    onChange(Object.fromEntries(free.map((i) => [i.id, on])));
  return (
    <Fold
      id={id}
      title={title}
      description={description}
      meta={`${items.filter((i) => i.on).length} of ${items.length} on`}
    >
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
    </Fold>
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
