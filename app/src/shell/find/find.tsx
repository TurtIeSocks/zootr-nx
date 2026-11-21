import { useMemo, useState } from "react";
import { commands } from "@/bindings";
import { Palette } from "@/components/palette/palette";
import { usePaletteKey } from "@/components/palette/use-palette-key";
import { message } from "@/lib/api";
import { NEED_TARGET } from "@/lib/config";
import { generateTyped } from "@/play/generate";
import {
  actions,
  checks,
  GROUPS,
  type Hit,
  pages,
  recents,
  search,
  seeds,
  settings,
} from "@/shell/find/sources";
import { useChecks } from "@/stores/checks";
import { useConfig, useDevTools } from "@/stores/config";
import {
  stale,
  stopGenerate,
  useBuild,
  useSeeds,
  useTargetRunning,
} from "@/stores/seeds";
import { useShell } from "@/stores/shell";
import { nextTheme, remember, useUi } from "@/stores/ui";
import { type Checked, checkNow } from "@/stores/update";

/** How many recent picks it keeps. */
const RECENT = 5;
/** The most results each group shows. */
const PER_GROUP = 5;

/** The recent picks list first, before anything is typed. */
const ALL_GROUPS = [{ id: "recent", title: "Recent" }, ...GROUPS];

/** What a check you asked for found, in words. */
const CHECKED: Record<Checked, string> = {
  newer: "A newer release is out: the sidebar has it",
  current: "Up to date",
  failed: "Couldn't check",
};

/** Starts the picked target's game, saying why it failed. */
async function playTarget(name: string | undefined) {
  if (!name) return;
  const result = await commands.play(name);
  return result.status === "error" ? message(result.error) : undefined;
}

/** Every hit, from the stores as they are now. */
function useHits(): Hit[] {
  const [schema] = useConfig.usePath("schema");
  const [groups] = useConfig.usePath("groups");
  const [targets] = useConfig.usePath("config.targets");
  const [choice] = useConfig.usePath("config.picked");
  const devTools = useDevTools();
  const [entries] = useSeeds.usePath("history.entries");
  const [installed] = useSeeds.usePath("history.installed");
  const [rows] = useChecks.usePath("rows");
  const [blocked] = useSeeds.usePath("blocked");
  const [unbuildable] = useSeeds.usePath("unbuildable");
  const [busy] = useSeeds.usePath("busy");
  const [generating] = useSeeds.usePath("generating");
  const [theme] = useUi.usePath("theme");
  const picked = choice ?? targets[0]?.name;
  const build = useBuild(picked);
  const running = useTargetRunning(picked);

  return useMemo(
    () => [
      ...pages(
        groups.map((g) => g.name),
        devTools,
        !!installed,
      ),
      ...settings(schema, devTools),
      ...actions({
        blocked,
        unbuildable,
        generating,
        busy,
        picked,
        cantPlay: !picked
          ? NEED_TARGET
          : running
            ? `${picked} is already running`
            : "",
        stale: stale(build),
        theme,
        generate: (install) =>
          generateTyped(install, install ? blocked : unbuildable, false),
        stop: () => void stopGenerate(),
        play: () => playTarget(picked),
        nextTheme,
        checkNow: async () =>
          CHECKED[await checkNow().catch(() => "failed" as const)],
      }),
      ...seeds(entries),
      ...(installed ? checks(rows) : []),
    ],
    [
      schema,
      groups,
      devTools,
      blocked,
      unbuildable,
      generating,
      busy,
      picked,
      running,
      build,
      theme,
      entries,
      installed,
      rows,
    ],
  );
}

const open = () => useShell.setPath("find", true);
const close = () => useShell.setPath("find", false);

/** The search over the whole app: pages, settings, actions, seeds and
 * checks. Opens on ⌘K (Ctrl+K) or the top bar's button. */
export function Find() {
  const [shown] = useShell.usePath("find");
  usePaletteKey(open);
  if (!shown) return null;
  return <FindDialog />;
}

function FindDialog() {
  const [query, setQuery] = useState("");
  /** What a run that's still open says, by hit id. */
  const [said, setSaid] = useState<Record<string, string>>({});
  const hits = useHits();
  const [recent] = useUi.usePath("find.recent");
  const found = query.trim() ? search(hits, query) : recents(hits, recent);
  const items = found.map((h) => (said[h.id] ? { ...h, body: said[h.id] } : h));

  const run = (h: Hit) => {
    if (h.muted) return;
    useUi.setPath("find.recent", (r) =>
      remember(
        r,
        h.found,
        RECENT,
        (a, b) => a.group === b.group && a.key === b.key,
      ),
    );
    const out = h.run();
    if (!out) return close();
    const say = (words: string) => setSaid((s) => ({ ...s, [h.id]: words }));
    say(h.pending ?? "…");
    void out.then((words) => (words ? say(words) : close()));
  };

  return (
    <Palette
      label="Search"
      placeholder="Find a page, setting, seed, check or action"
      hint="Type to find a page, setting, seed, check or action"
      query={query}
      onQuery={setQuery}
      groups={ALL_GROUPS}
      items={items}
      cap={PER_GROUP}
      runLabel="open"
      onRun={run}
      onClose={close}
      className="fixed inset-0 flex justify-center px-4 pt-[12vh]"
      panelClassName="h-fit w-full max-w-[600px]"
    />
  );
}
