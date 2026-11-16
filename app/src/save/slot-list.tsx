import { useState } from "react";
import type { Cell, SlotRow, Stat } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ItemIcon, Tile } from "@/items/tile";
import { ago, count } from "@/lib/format";
import { cn } from "@/lib/utils";
import { pendingCount, pickSlot, usePending } from "@/save/tabs";
import { useUi } from "@/stores/ui";

/** A stone or medallion's side in pixels, about a line of text tall. */
const GEM = 22;

/** Props for {@link SlotList}. */
interface SlotListProps {
  /** Each slot holding a save, newest first. */
  rows: SlotRow[];
}

/** The slots, one row each with what's in it. Picking another with edits
 * pending asks first. */
export function SlotList({ rows }: SlotListProps) {
  const [slot] = useUi.usePath("save.slot");
  const pending = pendingCount(usePending());
  const [asking, setAsking] = useState<SlotRow>();
  if (rows.length === 0) {
    return (
      <p className="text-ink-muted text-xs">No saves in the save folder yet</p>
    );
  }
  const pick = (row: SlotRow) =>
    row.slot === slot || pending === 0
      ? pickSlot(row.slot, row.stamp)
      : setAsking(row);
  const continues = rows.find((r) => r.continues);
  return (
    <>
      <div role="radiogroup" aria-label="Slot" className="flex flex-col gap-1">
        {rows.map((row) => (
          <SlotButton
            key={row.slot}
            row={row}
            picked={row.slot === slot}
            onPick={() => pick(row)}
          />
        ))}
      </div>
      {continues && slot !== null && continues.slot !== slot && (
        <p className="text-ink-muted text-xs">
          Continue loads slot {continues.slot}, the one written last.
        </p>
      )}
      <Dialog open={!!asking} onOpenChange={(o) => !o && setAsking(undefined)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Discard {count(pending, "change")}?</DialogTitle>
            <DialogDescription>
              They're for slot {slot}. Switching to slot {asking?.slot} drops
              them.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setAsking(undefined)}>
              Keep editing
            </Button>
            <Button
              onClick={() => {
                if (asking) pickSlot(asking.slot, asking.stamp);
                setAsking(undefined);
              }}
            >
              Discard and switch
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

/** Props for {@link SlotButton}. */
interface SlotButtonProps {
  row: SlotRow;
  picked: boolean;
  onPick: () => void;
}

/** One slot: when it was written and a short summary of the save. */
function SlotButton({ row, picked, onPick }: SlotButtonProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a row of facts, not a bare radio
    <button
      type="button"
      role="radio"
      aria-checked={picked}
      onClick={onPick}
      className={cn(
        "flex items-center gap-4 rounded-md px-3 py-2 text-left text-[13px] transition-colors duration-(--duration-fast)",
        picked
          ? "bg-primary-soft text-primary-ink"
          : "text-ink hover:bg-surface-sunken",
      )}
    >
      <span className="w-14 font-semibold">Slot {row.slot}</span>
      <span className="w-16 text-ink-muted">{ago(row.stamp)}</span>
      {row.error ? (
        <span className="text-danger-ink">{row.error}</span>
      ) : (
        <span className="flex flex-1 gap-4">
          {row.child !== null && <span>{row.child ? "Child" : "Adult"}</span>}
          <Gems cells={row.stones} />
          <Gems cells={row.medallions} />
          {row.stats.map((stat) => (
            <StatIcon key={stat.label} stat={stat} />
          ))}
        </span>
      )}
      {row.continues && <Badge variant="info">Continue</Badge>}
    </button>
  );
}

/** A number beside its icon, both greyed at none. */
function StatIcon({ stat }: { stat: Stat }) {
  return (
    <span
      title={stat.label}
      className={cn(
        "flex items-center gap-1 tabular-nums",
        stat.value === 0 && "opacity-28 grayscale",
      )}
    >
      <span
        className="flex items-center justify-center"
        style={{ width: GEM, height: GEM }}
      >
        <ItemIcon icon={stat.icon} glyph={stat.glyph} ink="text-ink" />
      </span>
      {(stat.value / stat.per).toLocaleString()}
      <span className="sr-only"> {stat.label}</span>
    </span>
  );
}

/** A row of stones or medallions, the missing ones greyed. */
function Gems({ cells }: { cells: Cell[] }) {
  return (
    <span className="flex items-center">
      {cells.map((cell) => (
        <Tile key={cell.label} cell={cell} size={GEM} ink="text-ink" />
      ))}
    </span>
  );
}
