import { useState } from "react";
import {
  type Cell,
  commands,
  type Info,
  type Intent,
  type ItemsView,
  type Placement,
  type Section,
  type StatusField,
  type StatusGroup,
} from "@/bindings";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { ChangedDot } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { BADGE, inSection, PIXELS } from "@/items/layout";
import { SectionGrid, Tile } from "@/items/tile";
import { call, message } from "@/lib/api";
import { cn } from "@/lib/utils";
import { StagedInput } from "@/save/staged-input";
import { stage } from "@/save/tabs";
import { useUi } from "@/stores/ui";

/** The editor's tiles, a fixed size on the page's own surface. */
const SIZE = PIXELS.Medium;
const INK = "text-ink";

/** The grid's columns, top to bottom, sized to come out about as tall as
 * each other. Dungeons is tall enough to take one alone. */
const COLUMNS: Section[][] = [
  ["Inventory", "Songs", "Quest"],
  ["Equipment", "Upgrades", "Stones", "Medallions"],
  ["Dungeons"],
];

/** The panel's groups, in order. */
const GROUPS: StatusGroup[] = ["Meters", "Gear", "Age", "Counters"];

/** Props for {@link ItemsTab}. */
interface ItemsTabProps {
  view: ItemsView;
}

/** Link's status, then his items as the tracker lays them out. A click on
 * a tile goes up a tier, a right click down. */
export function ItemsTab({ view }: ItemsTabProps) {
  const [error, setError] = useState<string>();
  const click = (at: number, intent: Intent) => {
    setError(undefined);
    clickCell(at, intent).catch((e) => setError(message(e)));
  };
  return (
    <div className="flex flex-col gap-4">
      <StatusPanel fields={view.status} />
      <InlineError error={error} />
      <Card>
        <div className="flex flex-wrap items-start gap-x-12 gap-y-5 p-4">
          {columns(view.sections).map((column) => (
            <div key={column[0]?.section} className="flex flex-col gap-5">
              {column.map((placement) => (
                <section key={placement.section}>
                  <SectionGrid
                    placement={placement}
                    cells={inSection(view.cells, placement.section)}
                    size={SIZE}
                    ink={INK}
                  >
                    {(cell, at) => (
                      <EditTile
                        cell={cell}
                        cap={view.caps[at] ?? null}
                        staged={view.staged[at] ?? false}
                        onClick={(intent) => click(at, intent)}
                      />
                    )}
                  </SectionGrid>
                </section>
              ))}
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}

/** The shown sections by {@link COLUMNS}, any it misses in the first. */
function columns(sections: Placement[]): Placement[][] {
  const shown = sections.filter((p) => p.shown);
  const out = COLUMNS.map((names) =>
    names.flatMap((name) => shown.filter((p) => p.section === name)),
  );
  const placed = new Set(COLUMNS.flat());
  out[0]?.push(...shown.filter((p) => !placed.has(p.section)));
  return out.filter((column) => column.length > 0);
}

/** Asks what a click on cell `at` changes, and stages it. */
async function clickCell(at: number, intent: Intent) {
  const { slot, presets, edits } = useUi.getState().save;
  if (slot === null) return;
  await stage(
    await call(commands.saveClick(slot, { presets, edits }, at, intent)),
  );
}

/** Props for {@link EditTile}. */
interface EditTileProps {
  cell: Cell;
  /** The most a counted cell holds. */
  cap: number | null;
  /** A pending edit changes it. */
  staged: boolean;
  onClick: (intent: Intent) => void;
}

/** A tile as a button, with its count as a button of its own. */
function EditTile({ cell, cap, staged, onClick }: EditTileProps) {
  const absent = cell.tiers === 0;
  return (
    <div className="relative">
      <button
        type="button"
        disabled={absent}
        title={`${cell.label}: click for the next, right click for the one before`}
        className="rounded-sm transition-colors duration-(--duration-fast) hover:bg-surface-sunken disabled:pointer-events-none"
        onClick={() => onClick("up")}
        onContextMenu={(e) => {
          e.preventDefault();
          onClick("down");
        }}
      >
        <Tile cell={cell} size={SIZE} ink={INK} noCount />
        {staged && <span className="sr-only"> (changed)</span>}
      </button>
      {cell.count !== null && cell.tier > 0 && (
        <CountButton cell={cell} cap={cap} onCount={onClick} />
      )}
      {staged && <ChangedDot className="absolute top-0.5 right-0.5 size-2" />}
    </div>
  );
}

/** Props for {@link CountButton}. */
interface CountButtonProps {
  cell: Cell;
  cap: number | null;
  onCount: (intent: Intent) => void;
}

/** The count on a tile, which opens a field for how many. */
function CountButton({ cell, cap, onCount }: CountButtonProps) {
  const [open, setOpen] = useState(false);
  const [draft, setDraft] = useState("");
  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (o) setDraft(String(cell.count ?? 0));
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`${cell.label}: ${cell.count}`}
          className={cn(BADGE, "cursor-pointer hover:bg-primary")}
        >
          {cell.count}
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-44 p-3">
        <form
          className="flex flex-col gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            onCount({ count: Number(draft) || 0 });
            setOpen(false);
          }}
        >
          <Input
            aria-label={`How many ${cell.label}`}
            type="number"
            min={0}
            max={cap ?? undefined}
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          {cap !== null && (
            <p className="text-ink-muted text-xs">Up to {cap}</p>
          )}
        </form>
      </PopoverContent>
    </Popover>
  );
}

/** Props for {@link StatusPanel}. */
interface StatusPanelProps {
  fields: StatusField[];
}

/** Every Status field the save holds, by group. */
function StatusPanel({ fields }: StatusPanelProps) {
  return (
    <Card>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-x-4 gap-y-3 p-4">
        {GROUPS.flatMap((group) =>
          fields
            .filter((f) => f.group === group)
            .map((f) => <StatusInput key={f.hash} field={f} />),
        )}
      </div>
    </Card>
  );
}

/** One Status field, shown in its own units: hearts for quarter hearts. */
function StatusInput({ field }: { field: StatusField }) {
  const per = field.per;
  const shown =
    per > 1 && field.value !== ""
      ? String(Number(field.value) / per)
      : field.value;
  const info: Info = {
    kind: per > 1 ? "Float" : "Int",
    input: field.input,
    choices: field.choices.map((c) => c.value),
    default: null,
  };
  const labels = Object.fromEntries(
    field.choices.map((c) => [c.value, c.label]),
  );
  const max = field.max !== null ? field.max / per : null;
  return (
    <div className="flex flex-col gap-1 text-[13px]">
      <span className="text-ink-muted text-xs">{field.label}</span>
      <StagedInput
        info={info}
        value={shown}
        label={field.label}
        labels={labels}
        onCommit={(v) =>
          stage([
            {
              hash: field.hash,
              index: 0,
              value:
                per > 1 && field.input === "number"
                  ? String(Math.round(Number(v) * per))
                  : v,
            },
          ])
        }
        note={
          field.warn ? (
            <span className="text-gold-ink text-xs">{field.warn}</span>
          ) : max !== null && field.input === "number" ? (
            <span className="text-ink-faint text-xs">Up to {max}</span>
          ) : null
        }
      />
    </div>
  );
}
