import { ArrowRight } from "lucide-react";
import type { Pick as ColorPick } from "@/bindings";
import { POOL_LABELS } from "@/lib/audio";
import {
  paletteKey,
  readPalettes,
  readPlando,
  readTracks,
} from "@/lib/catalogs";
import { PANE_OF, type Pane, type PaneChange } from "@/lib/drift";
import { checkLabels } from "@/lib/plando";
import type { Change } from "@/lib/settings";
import { type Loaded, useLoad } from "@/lib/use-load";
import { useConfig } from "@/stores/config";

/** Props for {@link ChangeList}. */
interface ChangeListProps {
  changes: Change[];
  /** The palettes, pools, tracks and pins that differ too, after the
   * settings. */
  panes?: PaneChange[];
}

/** Settings that differ, under their groups in schema order, then the
 * panes' entries under each pane, each as "Name  old → new" with the old
 * value struck through. */
export function ChangeList({ changes, panes = [] }: ChangeListProps) {
  const [groups] = useConfig.usePath("groups");
  return (
    <ul className="flex flex-col gap-0.5 text-[12.5px]">
      {groups.flatMap(({ name }) => {
        const mine = changes.filter((c) => c.def.group === name);
        if (mine.length === 0) return [];
        return [
          <Overline key={`group:${name}`}>{name}</Overline>,
          ...mine.map(({ def, before, after }) => (
            <Row key={def.id} label={def.label} before={before} after={after} />
          )),
        ];
      })}
      {panes.length > 0 && <PaneRows changes={panes} />}
    </ul>
  );
}

function Overline({ children }: { children: string }) {
  return <li className="type-overline mt-1.5">{children}</li>;
}

/** Props for {@link Row}. */
interface RowProps {
  label: string;
  before: string;
  after: string;
}

function Row({ label, before, after }: RowProps) {
  return (
    <li className="grid grid-cols-[1fr_auto_14px_auto] items-center gap-1.5 py-[3px]">
      <span>{label}</span>
      <s className="text-ink-muted decoration-ink-faint">{before}</s>
      <ArrowRight aria-label="to" className="size-3 text-ink-faint" />
      <span className="font-medium text-ink">{after}</span>
    </li>
  );
}

/** Each pane's title, in the order the settings list them. */
const PANES: [Pane, string][] = [
  ["colors", "Colors"],
  ["audio", "Audio"],
  ["plando", "Plando"],
];

/** A color pick in words. */
function pickWords(pick: ColorPick | undefined) {
  if (pick === undefined || pick === "vanilla") return "Vanilla";
  if (pick === "random") return "Random";
  return [pick.hex, ...(pick.split ?? [])].join(" ").toUpperCase();
}

const capital = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** Each pane's changes under its title, named from its catalog once that's
 * read, by id until then. A catalog is read only for a pane that changed. */
function PaneRows({ changes }: { changes: PaneChange[] }) {
  const names = usePaneNames(changes);
  return PANES.flatMap(([pane, title]) => {
    const mine = changes.filter((c) => PANE_OF[c.kind] === pane);
    if (mine.length === 0) return [];
    return [
      <Overline key={`pane:${pane}`}>{title}</Overline>,
      ...mine.map((c) => <Row key={`${c.kind}:${c.key}`} {...names(c)} />),
    ];
  });
}

const NONE = async () => null;

/** A load's data once in, else undefined. */
const ok = <T,>(l: Loaded<T | null>) =>
  l.status === "ok" ? (l.data ?? undefined) : undefined;

/** A change's label and values in words. */
function usePaneNames(changes: PaneChange[]) {
  const needs = (kind: PaneChange["kind"]) =>
    changes.some((c) => c.kind === kind);
  const [game] = useConfig.usePath("config.game");
  const [art] = useConfig.usePath("config.game_assets");
  const palettes = useLoad(
    needs("palette") ? readPalettes : NONE,
    paletteKey(game, art),
  );
  const tracks = useLoad(needs("track") ? readTracks : NONE, null);
  const plando = useLoad(needs("pin") ? readPlando : NONE, null);
  const palette = new Map(ok(palettes)?.map((p) => [p.id, p.label]));
  const track = new Map(ok(tracks)?.map((t) => [t.id, t.label]));
  const pins = ok(plando);
  const check = new Map(
    [...checkLabels(pins?.checks ?? [])].map(([label, c]) => [c.hash, label]),
  );
  const item = new Map(pins?.items.map((i) => [i.actor, i.name]));
  return (c: PaneChange): RowProps => {
    switch (c.kind) {
      case "palette":
        return {
          label: palette.get(c.key) ?? c.key,
          before: pickWords(c.before),
          after: pickWords(c.after),
        };
      case "pool": {
        const mode = (m?: string) => capital(m ?? "vanilla");
        return {
          label: `${POOL_LABELS[c.key as keyof typeof POOL_LABELS] ?? c.key} pool`,
          before: mode(c.before),
          after: mode(c.after),
        };
      }
      case "track": {
        const value = (v?: string) =>
          v === undefined
            ? "Its pool"
            : v === "vanilla"
              ? "Vanilla"
              : (track.get(v) ?? v);
        return {
          label: track.get(c.key) ?? c.key,
          before: value(c.before),
          after: value(c.after),
        };
      }
      case "pin": {
        const value = (v?: string) =>
          v === undefined ? "Not pinned" : (item.get(v) ?? v);
        return {
          label: check.get(c.key) ?? c.key,
          before: value(c.before),
          after: value(c.after),
        };
      }
    }
  };
}
