import { History, RotateCcw } from "lucide-react";
import type { SettingDef } from "@/bindings";
import { SplitButton, type SplitOption } from "@/components/split-button";
import { changesAnything, NO_PANES, type Pane, resetPane } from "@/lib/drift";
import { useRequest } from "@/lib/request";
import { defaults, isChanged, type Patch } from "@/lib/settings";
import { changeSettings, saveConfig, useConfig } from "@/stores/config";
import { useDriftBase, useSeeds } from "@/stores/seeds";
import { type ResetTo, useUi } from "@/stores/ui";

/** One way to reset: what it runs, and whether it would change anything. */
interface Reset {
  run: () => Promise<unknown>;
  changes: boolean;
}

/** How a page's resets read. */
interface ResetWords {
  /** What Reset to Defaults does. */
  defaults: string;
  /** What Reset to Last Seed does. */
  seed: string;
  /** What the page holds, for "Already the last seed's …". */
  noun: string;
}

/** Props for {@link ResetSplit}. */
interface ResetSplitProps {
  resets: Record<ResetTo, Reset>;
  words: ResetWords;
}

/** A page back to the defaults or to the last seed's, the one used last
 * leading (the defaults while there's no seed) and the other a click away
 * in its menu. The last seed is the one drift measures from. */
function ResetSplit({ resets, words }: ResetSplitProps) {
  const { run } = useRequest();
  const [lead, setLead] = useUi.usePath("settings.reset");
  const base = useDriftBase();
  const option = (to: ResetTo, o: Omit<SplitOption, "act">): SplitOption => ({
    ...o,
    act: () => {
      setLead(to);
      run(resets[to].run);
    },
  });
  const options: Record<ResetTo, SplitOption> = {
    defaults: option("defaults", {
      icon: RotateCcw,
      label: "Reset to Defaults",
      line: words.defaults,
      why: !resets.defaults.changes && "Already the defaults",
    }),
    seed: option("seed", {
      icon: History,
      label: "Reset to Last Seed",
      line: words.seed,
      why: base
        ? !resets.seed.changes && `Already the last seed's ${words.noun}`
        : "No seed built yet",
    }),
  };
  // With no seed built, Reset to Last Seed can't act at all, so the
  // defaults lead until there's one.
  const shown: ResetTo = lead === "seed" && !base ? "defaults" : lead;
  return (
    <SplitButton
      variant="ghost"
      options={Object.values(options)}
      lead={options[shown]}
      more="More ways to reset"
    />
  );
}

/** Props for {@link ResetButton}. */
export interface ResetButtonProps {
  /** The page's settings. */
  defs: SettingDef[];
}

/** A settings page's settings back to their defaults or the last seed's. */
export function ResetButton({ defs }: ResetButtonProps) {
  const changed = useConfig((s) =>
    defs.some((d) => isChanged(d, s.config.settings)),
  );
  const base = useDriftBase();
  const drifted = useSeeds((s) =>
    s.drift.some((c) => defs.some((d) => d.id === c.def.id)),
  );
  const seed: Patch = Object.fromEntries(
    defs.map((d) => [d.id, base?.settings[d.id] as Patch[string]]),
  );
  return (
    <ResetSplit
      resets={{
        defaults: {
          run: () => changeSettings(defaults(defs)),
          changes: changed,
        },
        seed: { run: () => changeSettings(seed), changes: drifted },
      }}
      words={{
        defaults: "Every setting here back to its default",
        seed: "Every setting here as the last seed had it",
        noun: "settings",
      }}
    />
  );
}

/** How each pane's resets read. */
const PANE_WORDS: Record<Pane, ResetWords> = {
  colors: {
    defaults: "Every unlocked palette back to Vanilla",
    seed: "Every unlocked palette as the last seed had it",
    noun: "colors",
  },
  audio: {
    defaults: "Every pool and track back to vanilla",
    seed: "Every pool and track as the last seed had it",
    noun: "music",
  },
  plando: {
    defaults: "Every pin taken off, so every check shuffles",
    seed: "The pins the last seed had",
    noun: "pins",
  },
};

/** Props for {@link PaneResetButton}. */
interface PaneResetButtonProps {
  pane: Pane;
}

/** A pane's draft back to none set or to the last seed's. Locked palettes
 * keep their picks. */
export function PaneResetButton({ pane }: PaneResetButtonProps) {
  const config = useConfig((s) => s.config);
  const base = useDriftBase();
  const reset = (to: typeof NO_PANES): Reset => {
    const change = resetPane(pane, to);
    return {
      run: () => saveConfig(change),
      changes: changesAnything(change, config),
    };
  };
  return (
    <ResetSplit
      resets={{ defaults: reset(NO_PANES), seed: reset(base ?? NO_PANES) }}
      words={PANE_WORDS[pane]}
    />
  );
}
