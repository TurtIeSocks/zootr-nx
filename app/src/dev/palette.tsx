import { type LucideIcon, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { Palette as SharedPalette } from "@/components/palette/palette";
import { area, spots } from "@/dev/areas";
import { openTab, TABS } from "@/dev/nav";
import { apply, give, warpTo } from "@/dev/requests";
import { matching } from "@/lib/matching";
import { useCanSend, useDev } from "@/stores/dev";
import { type DevTab, useUi } from "@/stores/ui";

/** The most results each group shows. */
const PER_GROUP = 4;

/** A result: what it shows, what Enter does, and what Tab does. */
interface Hit {
  tab: Exclude<DevTab, "actors">;
  label: string;
  /** Enter sends a request, so it waits for the game. */
  sends: boolean;
  run: () => void;
}

/** The groups, in order, with their headings. */
const GROUPS: { id: Hit["tab"]; title: string }[] = [
  { id: "warp", title: "Warp" },
  { id: "give", title: "Give" },
  { id: "values", title: "Values" },
  { id: "tests", title: "Tests" },
];

const icon = (tab: DevTab): LucideIcon =>
  TABS.find((t) => t.id === tab)?.icon ?? Search;

/** The first `PER_GROUP` of `labels` holding every word of `query`. */
const top = (labels: string[], query: string) =>
  matching(labels, query, PER_GROUP);

/** Every result for `query`: places for Link's age, items, GameData names
 * and saved setups. */
function useHits(query: string): Hit[] {
  const [options] = useDev.usePath("options");
  const [child] = useDev.usePath("game.state.child");
  const [tests] = useDev.usePath("tests.names");

  return useMemo(() => {
    const places = new Map(
      spots(options.places, child, "").map((s) => [
        `${area(s.map)} · ${s.spot}`,
        s,
      ]),
    );
    return [
      ...top([...places.keys()], query).map((label) => ({
        tab: "warp" as const,
        label,
        sends: true,
        run: () => {
          const spot = places.get(label);
          if (spot) void warpTo(spot);
        },
      })),
      ...top(options.items, query).map((label) => ({
        tab: "give" as const,
        label,
        sends: true,
        run: () => void give(label, 1),
      })),
      ...top(options.names, query).map((label) => ({
        tab: "values" as const,
        label,
        sends: false,
        run: () => {
          useUi.setPath("dev.set.name", label);
          openTab("values");
        },
      })),
      ...top(tests ?? [], query).map((label) => ({
        tab: "tests" as const,
        label,
        sends: true,
        run: () => void apply(label),
      })),
    ];
  }, [options, child, tests, query]);
}

/** Opens `tab` with `query` in its search. */
function openWith(tab: Hit["tab"], query: string) {
  if (tab === "warp") {
    useUi.setPath("dev.teleport.tab", "places");
    useUi.setPath("dev.teleport.places", query);
  } else if (tab === "give") useUi.setPath("dev.give.item", query);
  else if (tab === "values") useUi.setPath("dev.set.name", query);
  openTab(tab);
}

/** Find anything: one search over places, items, names and setups. Enter
 * runs the highlighted result, Tab opens it in its tab, Esc closes. */
export function Palette() {
  const [open] = useDev.usePath("palette");
  if (!open) return null;
  return <PaletteDialog />;
}

const close = () => useDev.setPath("palette", false);

function PaletteDialog() {
  const [query, setQuery] = useState("");
  const hits = useHits(query);
  const can = useCanSend();
  const items = hits.map((hit, i) => ({
    ...hit,
    id: `${hit.tab}/${i}`,
    group: hit.tab,
    icon: icon(hit.tab),
    title: hit.label,
    muted: hit.sends && !can,
  }));

  return (
    <SharedPalette
      label="Find anything"
      placeholder="Place, item, value or test"
      hint="Type to find a place, item, GameData name or test setup"
      query={query}
      onQuery={setQuery}
      groups={GROUPS}
      items={items}
      className="absolute inset-0 p-3"
      keys={[{ keys: ["⇥"], label: "open in its tab" }]}
      onClose={close}
      onRun={(hit) => {
        if (hit.muted) return;
        hit.run();
        close();
      }}
      onKey={(key, hit) => {
        if (key !== "Tab") return false;
        if (hit) {
          openWith(hit.tab, query);
          close();
        }
        return true;
      }}
    />
  );
}
