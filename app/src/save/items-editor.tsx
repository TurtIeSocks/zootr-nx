import { useState } from "react";
import type {
  Cell,
  Edit,
  Info,
  Intent,
  ItemsView,
  Placement,
  Section,
  StatusField,
  StatusGroup,
} from "@/bindings";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Tip } from "@/components/tip";
import { ChangedDot } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { BADGE, inSection, PIXELS } from "@/items/layout";
import { SectionGrid, Tile } from "@/items/tile";
import { message } from "@/lib/api";
import { cn } from "@/lib/utils";
import { StagedInput } from "@/save/staged-input";

/** The editor's tiles, a fixed size on the page's own surface. */
const SIZE = PIXELS.Medium;
const INK = "text-ink";

/** The item columns, top to bottom, sized to come out about as tall as
 * each other. They sit side by side where there's room, else stacked. */
const MAIN: Section[][] = [
  ["Inventory", "Songs", "Upgrades"],
  ["Equipment", "Quest", "Stones", "Medallions"],
];

/** Tall enough to take a column alone, kept on the right at any width. */
const SIDE: Section[] = ["Dungeons"];

/** The panel's groups, in order. */
const GROUPS: StatusGroup[] = ["Meters", "Gear", "Age", "Counters"];

/** Props for {@link ItemsEditor}. */
export interface ItemsEditorProps {
  view: ItemsView;
  /** Takes a click on cell `at`. A throw shows above the grid. */
  onClick: (at: number, intent: Intent) => Promise<void>;
  /** Takes a Status field's new value. A throw shows under the field. */
  onStatus: (edit: Edit) => Promise<void>;
}

/** Link's status, then his items as the tracker lays them out. A click on
 * a tile goes up a tier, a right click down. The Save page and the
 * settings' starting inventory both draw it. */
export function ItemsEditor({ view, onClick, onStatus }: ItemsEditorProps) {
  const [error, setError] = useState<string>();
  const side = view.sections.filter((p) => p.shown && SIDE.includes(p.section));
  const click = (at: number, intent: Intent) => {
    setError(undefined);
    onClick(at, intent).catch((e) => setError(message(e)));
  };
  return (
    <div className="flex flex-col gap-4">
      {view.status.length > 0 && (
        <StatusPanel fields={view.status} onCommit={onStatus} />
      )}
      <InlineError error={error} />
      <Card>
        <div className="flex items-start gap-x-12 p-4">
          <div className="flex flex-wrap items-start gap-x-12 gap-y-5">
            {main(view.sections).map((column) => (
              <Column
                key={column[0]?.section}
                view={view}
                placements={column}
                onClick={click}
              />
            ))}
          </div>
          {side.length > 0 && (
            <Column view={view} placements={side} onClick={click} />
          )}
        </div>
      </Card>
    </div>
  );
}

/** The shown sections by {@link MAIN}, any it and {@link SIDE} miss in
 * the first. */
function main(sections: Placement[]): Placement[][] {
  const shown = sections.filter((p) => p.shown);
  const out = MAIN.map((names) =>
    names.flatMap((name) => shown.filter((p) => p.section === name)),
  );
  const placed = new Set([...MAIN.flat(), ...SIDE]);
  out[0]?.push(...shown.filter((p) => !placed.has(p.section)));
  return out.filter((column) => column.length > 0);
}

/** Props for {@link Column}. */
interface ColumnProps {
  view: ItemsView;
  placements: Placement[];
  onClick: (at: number, intent: Intent) => void;
}

/** Sections stacked top to bottom, their tiles as buttons. */
function Column({ view, placements, onClick }: ColumnProps) {
  return (
    <div className="flex flex-col gap-5">
      {placements.map((placement) => (
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
                onClick={(intent) => onClick(at, intent)}
              />
            )}
          </SectionGrid>
        </section>
      ))}
    </div>
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
      <Tip
        tip={`${cell.label}: click for the next, right click for the one before`}
      >
        <button
          type="button"
          disabled={absent}
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
      </Tip>
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

/** Props for {@link StatusPanel} and {@link StatusInput}. */
interface CommitProps {
  onCommit: (edit: Edit) => Promise<void>;
}

/** Props for {@link StatusPanel}. */
interface StatusPanelProps extends CommitProps {
  fields: StatusField[];
}

/** Every Status field the view holds, by group. */
function StatusPanel({ fields, onCommit }: StatusPanelProps) {
  return (
    <Card>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-x-4 gap-y-3 p-4">
        {GROUPS.flatMap((group) =>
          fields
            .filter((f) => f.group === group)
            .map((f) => (
              <StatusInput key={f.hash} field={f} onCommit={onCommit} />
            )),
        )}
      </div>
    </Card>
  );
}

/** One Status field, shown in its own units: hearts for quarter hearts. */
function StatusInput({
  field,
  onCommit,
}: CommitProps & { field: StatusField }) {
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
          onCommit({
            hash: field.hash,
            index: 0,
            value:
              per > 1 && field.input === "number"
                ? String(Math.round(Number(v) * per))
                : v,
          })
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
