import type {
  AudioDraft,
  Pick as ColorPick,
  Pool,
  PoolMode,
  Summary,
} from "@/bindings";
import { same } from "@/lib/api";
import { tidy } from "@/lib/audio";
import type { FullConfig } from "@/lib/config";
import type { Change } from "@/lib/saved-sets";

/** The panes beside the settings whose drafts drift counts. */
export type Pane = "colors" | "audio" | "plando";

/** What the panes' drafts are measured from: a seed's, as asked. */
export type PaneBase = Pick<Summary, "colors" | "audio" | "place">;

/** The panes with nothing set: what the defaults are. */
export const NO_PANES: PaneBase = { colors: {}, audio: {}, place: {} };

/** One pane entry that differs from the base: a palette, a pool, a track
 * override or a pin, as it was and is. Unset is undefined. */
export type PaneChange =
  | { kind: "palette"; key: string; before?: ColorPick; after?: ColorPick }
  | { kind: "pool"; key: Pool; before?: PoolMode; after?: PoolMode }
  | { kind: "track"; key: string; before?: string; after?: string }
  | { kind: "pin"; key: string; before?: string; after?: string };

/** The pane each kind of change is on. */
export const PANE_OF: Record<PaneChange["kind"], Pane> = {
  palette: "colors",
  pool: "audio",
  track: "audio",
  pin: "plando",
};

/** `rec` without the entries that say what an unset one means. */
const unset = <T>(rec: Record<string, T> | undefined, value: T) =>
  Object.fromEntries(Object.entries(rec ?? {}).filter(([, v]) => v !== value));

/** Each key whose value differs between `a` and `b`, sorted, with both. */
function differ<T>(a: Record<string, T>, b: Record<string, T>) {
  const keys = [...new Set([...Object.keys(a), ...Object.keys(b)])].sort();
  return keys
    .filter((k) => !same(a[k], b[k]))
    .map((key) => ({ key, before: a[key], after: b[key] }));
}

/** Every palette, pool, track and pin `config` sets differently from
 * `base`. Vanilla palettes and pools count as unset, as the backend reads
 * them. */
export function paneChanges(base: PaneBase, config: FullConfig): PaneChange[] {
  const pools = (d: AudioDraft) => unset(d.pools, "vanilla" as PoolMode);
  return [
    ...differ(
      unset(base.colors, "vanilla" as ColorPick),
      unset(config.colors, "vanilla" as ColorPick),
    ).map((c) => ({ kind: "palette" as const, ...c })),
    ...differ(pools(base.audio), pools(config.audio)).map((c) => ({
      kind: "pool" as const,
      ...c,
      key: c.key as Pool,
    })),
    ...differ(base.audio.tracks ?? {}, config.audio.tracks ?? {}).map((c) => ({
      kind: "track" as const,
      ...c,
    })),
    ...differ(base.place, config.place).map((c) => ({
      kind: "pin" as const,
      ...c,
    })),
  ];
}

/** Pane `pane`'s draft back to `base`'s. A locked palette keeps its pick:
 * locks hold against every reset. */
export function resetPane(pane: Pane, base: PaneBase): Change {
  switch (pane) {
    case "colors":
      return (c) => {
        const colors = { ...base.colors };
        for (const id of c.color_locks) {
          if (c.colors[id] === undefined) delete colors[id];
          else colors[id] = c.colors[id];
        }
        return { colors: unset(colors, "vanilla" as ColorPick) };
      };
    case "audio":
      return () => ({ audio: tidy(base.audio) });
    case "plando":
      return () => ({ place: { ...base.place } });
  }
}

/** Whether `change` would change anything in `config`. */
export const changesAnything = (change: Change, config: FullConfig) =>
  Object.entries(change(config)).some(
    ([k, v]) =>
      !same(
        normal(v),
        normal(config[k as keyof FullConfig] as Record<string, unknown>),
      ),
  );

/** `rec` with its keys sorted, so two holding the same compare the same. */
const normal = (rec: unknown) =>
  rec && typeof rec === "object" && !Array.isArray(rec)
    ? Object.fromEntries(
        Object.entries(rec).sort(([a], [b]) => (a < b ? -1 : 1)),
      )
    : rec;
