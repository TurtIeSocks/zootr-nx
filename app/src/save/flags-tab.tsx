import { ChevronDown, ChevronRight, RotateCcw } from "lucide-react";
import { useState } from "react";
import type { FlagGroup, FlagRow, FlagsView, Info } from "@/bindings";
import { Card } from "@/components/card";
import { divided, fieldClass } from "@/components/styles";
import { ChangedDot } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { StagedInput } from "@/save/staged-input";
import { stage } from "@/save/tabs";
import { useUi } from "@/stores/ui";

/** Rows an open group adds with each Show more, as Rust pages them. */
const PAGE = 200;

/** Props for {@link FlagsTab}. */
interface FlagsTabProps {
  view: FlagsView;
}

/** Every value in the save, grouped by name and filtered. */
export function FlagsTab({ view }: FlagsTabProps) {
  return (
    <div className="flex flex-col gap-3">
      <FlagFilters kinds={view.kinds} />
      {view.groups.length === 0 ? (
        <p className="text-ink-muted text-xs">Nothing matches</p>
      ) : (
        <Card>
          <div className={cn("flex flex-col", divided)}>
            {view.groups.map((g) => (
              <Group key={g.name} group={g} />
            ))}
          </div>
        </Card>
      )}
    </div>
  );
}

/** Search, the type, and the Changed and Staged filters. */
function FlagFilters({ kinds }: { kinds: string[] }) {
  const [search, setSearch] = useUi.usePath("save.flags.search");
  const [kind, setKind] = useUi.usePath("save.flags.kind");
  const [changed, setChanged] = useUi.usePath("save.flags.changed");
  const [staged, setStaged] = useUi.usePath("save.flags.staged");
  return (
    <div className="flex items-center gap-3">
      <Input
        aria-label="Search"
        placeholder="Search a name, hash or label"
        className="flex-1"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      <select
        aria-label="Type"
        className={cn(fieldClass, "h-8 w-32 px-2")}
        value={kind ?? ""}
        onChange={(e) => setKind(e.target.value || null)}
      >
        <option value="">Every type</option>
        {kinds.map((k) => (
          <option key={k} value={k}>
            {k}
          </option>
        ))}
      </select>
      <Toggle label="Changed" checked={changed} onChange={setChanged} />
      <Toggle label="Staged" checked={staged} onChange={setStaged} />
    </div>
  );
}

/** Props for {@link Toggle}. */
interface ToggleProps {
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
}

/** A labelled checkbox filter. */
function Toggle({ label, checked, onChange }: ToggleProps) {
  return (
    // biome-ignore lint/a11y/noLabelWithoutControl: the checkbox is inside
    <label className="flex items-center gap-1.5 text-[13px]">
      <Checkbox
        checked={checked}
        onCheckedChange={(c) => onChange(c === true)}
      />
      {label}
    </label>
  );
}

/** Opens or closes a group, or shows a page more of it. */
const setOpen = (name: string, rows: number | null) =>
  useUi.setPath("save.flags.open", (open) => {
    const { [name]: _, ...rest } = open;
    return rows === null ? rest : { ...rest, [name]: rows };
  });

/** A group's header, and its rows when open. */
function Group({ group }: { group: FlagGroup }) {
  const Chevron = group.open ? ChevronDown : ChevronRight;
  const shown = group.rows.length;
  return (
    <div className="relative">
      <button
        type="button"
        aria-expanded={group.open}
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-[13px] hover:bg-surface-sunken"
        onClick={() => setOpen(group.name, group.open ? null : PAGE)}
      >
        <Chevron className="size-3.5 text-ink-muted" />
        <span className="flex-1 font-medium">{group.name}</span>
        <span className="text-ink-muted text-xs">
          {group.count.toLocaleString()}
          {group.changed > 0 && ` (${group.changed} changed)`}
        </span>
      </button>
      {group.open && (
        <div className="flex flex-col pb-2">
          {group.rows.map((r) => (
            <Row key={r.hash} row={r} />
          ))}
          {shown < group.count && (
            <div className="px-9 pt-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setOpen(group.name, shown + PAGE)}
              >
                {shown.toLocaleString()} of {group.count.toLocaleString()}{" "}
                shown. Show more
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** A row's field, for one element. */
const infoOf = (row: FlagRow): Info => ({
  kind: row.kind.replace(/Array$/, ""),
  input: row.input,
  choices: row.choices,
  default: row.default,
});

/** One value: its name and type, then its field, or one per element for
 * an array once opened. */
function Row({ row }: { row: FlagRow }) {
  const [open, setOpenRow] = useState(false);
  const array = row.kind.endsWith("Array");
  const edit = (index: number) => (value: string) =>
    stage([{ hash: row.hash, index, value }]);
  const resettable =
    row.changed && row.default !== null && row.input !== "read" && !array;
  return (
    <div className="flex flex-col gap-1 py-1 pr-3 pl-9 text-[13px]">
      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1 truncate font-mono" title={row.name}>
          {row.name}
          {row.label && (
            <span className="ml-2 font-sans text-ink-muted">{row.label}</span>
          )}
        </span>
        {row.staged && (
          <>
            <ChangedDot className="size-2" />
            <span className="sr-only">Staged</span>
          </>
        )}
        <span className="w-24 shrink-0 text-ink-muted text-xs">{row.kind}</span>
        <div className="w-56 shrink-0">
          {row.values.length === 0 ? (
            <span className="text-ink-muted text-xs">
              {row.size} bytes, read only
            </span>
          ) : array ? (
            <Button variant="ghost" size="sm" onClick={() => setOpenRow(!open)}>
              {open ? "Hide" : "Show"} {row.values.length} elements
            </Button>
          ) : (
            <StagedInput
              info={infoOf(row)}
              value={row.values[0] ?? ""}
              label={row.name}
              onCommit={edit(0)}
            />
          )}
        </div>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Reset ${row.name}`}
          title={`Back to ${row.default}`}
          className={cn(!resettable && "invisible")}
          onClick={() => row.default !== null && edit(0)(row.default)}
        >
          <RotateCcw />
        </Button>
      </div>
      {array && open && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2 pl-4">
          {row.values.map((v, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: an element is its position
            <div key={i} className="flex flex-col gap-0.5">
              <span className="text-ink-muted text-xs">[{i}]</span>
              <StagedInput
                info={infoOf(row)}
                value={v}
                label={`${row.name}[${i}]`}
                onCommit={edit(i)}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
