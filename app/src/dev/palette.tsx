import { type LucideIcon, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { area, spots } from "@/dev/areas";
import { openTab, TABS } from "@/dev/nav";
import { apply, give, warpTo } from "@/dev/requests";
import { matching } from "@/lib/matching";
import { cn } from "@/lib/utils";
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
const GROUPS: { tab: Hit["tab"]; title: string }[] = [
  { tab: "warp", title: "Warp" },
  { tab: "give", title: "Give" },
  { tab: "values", title: "Values" },
  { tab: "tests", title: "Tests" },
];

const icon = (tab: DevTab): LucideIcon =>
  TABS.find((t) => t.id === tab)?.icon ?? Search;

/** The first `PER_GROUP` of `labels` containing `query`, any case. */
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
  const [active, setActive] = useState(0);
  const hits = useHits(query);
  const can = useCanSend();
  const at = Math.min(active, Math.max(hits.length - 1, 0));

  const run = (hit: Hit | undefined) => {
    if (!hit || (hit.sends && !can)) return;
    hit.run();
    close();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const hit = hits[at];
    if (e.key === "ArrowDown") setActive(Math.min(at + 1, hits.length - 1));
    else if (e.key === "ArrowUp") setActive(Math.max(at - 1, 0));
    else if (e.key === "Enter") run(hit);
    else if (e.key === "Tab") {
      if (hit) {
        openWith(hit.tab, query);
        close();
      }
    } else if (e.key === "Escape") close();
    else return;
    e.preventDefault();
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the scrim closes on a click; Esc closes from the keyboard
    <div
      className="absolute inset-0 z-50 bg-scrim p-3"
      onMouseDown={(e) => e.target === e.currentTarget && close()}
    >
      <div
        role="dialog"
        aria-modal
        aria-label="Find anything"
        className="overflow-hidden rounded-xl bg-surface-raised shadow-lg"
      >
        <div className="flex items-center gap-2 border-line border-b px-3">
          <Search aria-hidden className="size-4 text-ink-faint" />
          <input
            // biome-ignore lint/a11y/noAutofocus: the palette opens to type in
            autoFocus
            aria-label="Find anything"
            placeholder="Place, item, value or test"
            className="h-11 min-w-0 flex-1 bg-transparent text-[14px] outline-none"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
          />
        </div>
        <Results hits={hits} at={at} query={query} can={can} onRun={run} />
        <Keys />
      </div>
    </div>
  );
}

/** The hits by group, the highlighted one marked. */
function Results({
  hits,
  at,
  query,
  can,
  onRun,
}: {
  hits: Hit[];
  at: number;
  query: string;
  can: boolean;
  onRun: (hit: Hit) => void;
}) {
  if (!query.trim())
    return (
      <p className="px-3 py-3 text-[13px] text-ink-faint">
        Type to find a place, item, GameData name or test setup
      </p>
    );
  if (hits.length === 0)
    return <p className="px-3 py-3 text-[13px] text-ink-faint">No matches</p>;
  return (
    <div className="max-h-[60vh] overflow-auto py-1">
      {GROUPS.map(({ tab, title }) => {
        const group = hits.filter((h) => h.tab === tab);
        if (group.length === 0) return null;
        const Icon = icon(tab);
        return (
          <div key={tab}>
            <h3 className="px-3 pt-2 pb-1 font-semibold text-[11px] text-ink-faint uppercase tracking-wide">
              {title}
            </h3>
            <ul aria-label={title}>
              {group.map((hit) => {
                const i = hits.indexOf(hit);
                return (
                  <li key={hit.label}>
                    <button
                      type="button"
                      tabIndex={-1}
                      aria-current={i === at}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => onRun(hit)}
                      className={cn(
                        "mx-1 flex w-[calc(100%-0.5rem)] cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-surface-hover",
                        i === at && "bg-primary-soft hover:bg-primary-soft",
                        hit.sends && !can && "text-ink-muted",
                      )}
                    >
                      <Icon
                        aria-hidden
                        className={cn(
                          "size-4 shrink-0 text-ink-faint",
                          i === at && "text-primary-ink",
                        )}
                      />
                      <Marked text={hit.label} query={query} />
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        );
      })}
    </div>
  );
}

/** `text` with its first match of `query` bolded. */
function Marked({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  const at = text.toLowerCase().indexOf(q.toLowerCase());
  if (!q || at < 0) return <span className="truncate">{text}</span>;
  return (
    <span className="truncate">
      {text.slice(0, at)}
      <b className="font-semibold">{text.slice(at, at + q.length)}</b>
      {text.slice(at + q.length)}
    </span>
  );
}

/** What each key does. */
function Keys() {
  const kbd =
    "rounded-xs border border-line-strong px-1 font-sans text-[11px] text-ink-muted";
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-line border-t px-3 py-2 text-[12px] text-ink-faint">
      <span>
        <kbd className={kbd}>↑</kbd> <kbd className={kbd}>↓</kbd> move
      </span>
      <span>
        <kbd className={kbd}>↵</kbd> run
      </span>
      <span>
        <kbd className={kbd}>⇥</kbd> open in its tab
      </span>
      <span className="ml-auto">
        <kbd className={kbd}>esc</kbd> close
      </span>
    </div>
  );
}
