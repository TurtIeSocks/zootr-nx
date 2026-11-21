import {
  CircleStop,
  ListChecks,
  ListPlus,
  type LucideIcon,
  Play,
  RefreshCw,
  ScrollText,
  Sparkles,
  SunMoon,
} from "lucide-react";
import type { Row, SettingDef, Summary } from "@/bindings";
import { areaOf, areas, type Filter, placeOf } from "@/checks/rows";
import type { PaletteItem } from "@/components/palette/palette";
import { GROUP_HELP, TOPICS, type Topic } from "@/help/topics";
import { nameOf } from "@/lib/format";
import { hasAll, words } from "@/lib/matching";
import { APP_FIELDS } from "@/settings/app-fields";
import { controlId, untested } from "@/settings/setting";
import { ADVANCED_FIELDS, FIELDS, shown } from "@/settings/targets-draft";
import { reveal, rowId } from "@/shell/find/reveal";
import {
  APP_PANES,
  groupIcon,
  PLAY_FOLDERS,
  RANDOMIZER_PANES,
  RUN_TABS,
  shows,
  TOP_TABS,
  tabLabel,
} from "@/shell/nav";
import { go, type Place } from "@/stores/shell";
import {
  type Found,
  type HistoryFilter,
  setSectionOpen,
  useUi,
} from "@/stores/ui";

/** The search's groups, in the order they list. */
export const GROUPS = [
  { id: "pages", title: "Pages" },
  { id: "settings", title: "Settings" },
  { id: "actions", title: "Actions" },
  { id: "seeds", title: "Seeds" },
  { id: "checks", title: "Checks" },
] as const;

/** A group's id. */
export type FindGroup = (typeof GROUPS)[number]["id"];

/** One result: what the palette shows, and what opening it does. A run
 * that returns a promise keeps the palette open, saying `pending`, and
 * shows the words it settles with, or closes on none. */
export interface Hit extends PaletteItem {
  /** Its own group, or "recent" while it lists as a recent pick. */
  group: FindGroup | "recent";
  /** Its own group and a key stable across launches: the recent picks
   * keep it. */
  found: Found;
  /** More words it matches on, not shown. */
  words?: string;
  /** Why it can't run now, shown in place of its line. */
  why?: string;
  pending?: string;
  // biome-ignore lint/suspicious/noConfusingVoidType: most runs return nothing; one that returns a promise keeps the palette open
  run: () => void | Promise<string | undefined>;
}

/** What a source says of a hit. `hit()` fills in the rest. */
type HitInput = Omit<Hit, "id" | "muted" | "found" | "group"> & {
  group: FindGroup;
  key: string;
};

/** A hit, its id and greying from its group, key and `why`. */
const hit = ({ group, key, ...h }: HitInput): Hit => ({
  ...h,
  group,
  found: { group, key },
  id: `${group}:${key}`,
  body: h.why || h.body,
  muted: !!h.why,
});

/** The words a topic says, as one line. */
const said = (t: Topic) => t.body.join(" ");

/** Whether two places are the same page. */
const samePlace = (a: Place, b: Place) =>
  "section" in a
    ? "section" in b && a.section === b.section
    : "tab" in b && a.tab === b.tab && a.folder === b.folder;

/** The topics that sit on `place`. */
const topicsAt = (place: Place) =>
  Object.values(TOPICS as Record<string, Topic>).filter(
    (t) => t.at && samePlace(t.at, place),
  );

/** A page's hit: its first topic as its line, and every topic there
 * matched on. */
function page(
  key: string,
  place: Place,
  trail: string[],
  title: string,
  icon: LucideIcon,
  topics: Topic[] = topicsAt(place),
): Hit {
  return hit({
    group: "pages",
    key,
    trail,
    title,
    icon,
    body: topics[0] && said(topics[0]),
    words: topics.map(said).join(" "),
    run: () => go(place),
  });
}

/** Every page the sidebar shows: the tabs (the run's only with a seed
 * installed), Play's folders, the app's panes and the randomizer's groups
 * and panes. */
export function pages(
  groups: string[],
  devTools: boolean,
  installed: boolean,
): Hit[] {
  return [
    ...TOP_TABS.map(([tab, icon]) =>
      page(tab, { tab }, [], tabLabel(tab), icon),
    ),
    ...Object.entries(PLAY_FOLDERS).map(([folder, { label, icon }]) =>
      page(
        `Play/${folder}`,
        { tab: "Play", folder: folder as keyof typeof PLAY_FOLDERS },
        ["Play"],
        label,
        icon,
      ),
    ),
    ...(installed ? RUN_TABS : []).map(([tab, icon]) =>
      page(tab, { tab }, [], tabLabel(tab), icon),
    ),
    ...APP_PANES.filter(([id]) => shows(id, devTools)).map(
      ([section, label, icon]) =>
        page(section, { section }, ["App"], label, icon),
    ),
    ...groups.map((g) =>
      page(
        g,
        { section: g },
        ["Randomizer"],
        g,
        groupIcon(g),
        GROUP_HELP[g] ? [GROUP_HELP[g]] : [],
      ),
    ),
    ...RANDOMIZER_PANES.map(([section, label, icon]) =>
      page(section, { section }, ["Randomizer"], label, icon),
    ),
  ];
}

/** Goes to `place`, then reveals the control `id`. */
const goReveal = (place: Place, id: string) => () => {
  go(place);
  reveal(id);
};

/** A field's help topic, as its line. */
const topicText = (id: keyof typeof TOPICS | undefined) =>
  id && said(TOPICS[id]);

/** Every setting: the schema's, by group, and the app's own fields. */
export function settings(schema: SettingDef[], devTools: boolean): Hit[] {
  const icon = (section: string) =>
    APP_PANES.find(([id]) => id === section)?.[2] ?? ListChecks;
  return [
    ...schema.map((d) =>
      hit({
        group: "settings",
        key: d.id,
        trail: ["Randomizer", d.group],
        title: d.label,
        icon: groupIcon(d.group),
        body: untested(d.description)[1],
        run: () => {
          go({ section: d.group });
          if (d.kind.type !== "inventory")
            setSectionOpen(`${d.group}/${d.section}`, true);
          reveal(controlId(d));
        },
      }),
    ),
    ...Object.values(APP_FIELDS).map((f) =>
      hit({
        group: "settings",
        key: f.id,
        trail: ["App", "Settings"],
        title: f.label,
        icon: icon("settings"),
        body: ("help" in f && topicText(f.help)) || f.description,
        run: goReveal({ section: "settings" }, f.id),
      }),
    ),
    // The first target's card stands for them all.
    ...shown([...FIELDS, ...ADVANCED_FIELDS], devTools).map((f) =>
      hit({
        group: "settings",
        key: `target/${f.key}`,
        trail: ["App", "Targets"],
        title: f.label,
        icon: icon("targets"),
        body: topicText(f.help) || f.hint,
        run: goReveal({ section: "targets" }, `0-${f.key}`),
      }),
    ),
  ];
}

/** Opens History with `id` showing, then reveals its row. */
function openSeed(id: number, filter: HistoryFilter, ap: boolean) {
  if (filter !== "all" && (filter === "archipelago") !== ap)
    useUi.setPath("play.history", "all");
  go({ tab: "Play", folder: "history" });
  reveal(rowId.seed(id));
}

/** Every seed in History, by its name and note. */
export function seeds(entries: Summary[]): Hit[] {
  return entries.map((e) =>
    hit({
      group: "seeds",
      key: String(e.id),
      trail: ["History"],
      title: nameOf(e) || `Seed ${e.seed}`,
      icon: ScrollText,
      body: [`#${e.id}`, e.label.note].filter(Boolean).join(" · "),
      run: () => openSeed(e.id, useUi.getPath("play.history"), !!e.ap_slot),
    }),
  );
}

/** Opens Checks with `row` showing, then reveals it. */
function openCheck(rows: Row[], row: Row) {
  const query = useUi.getPath("checks.query");
  const filter: Filter = useUi.getPath("checks.filter");
  if (!areas(rows, query, filter).some((a) => a.rows.includes(row))) {
    useUi.setPath("checks.query", "");
    useUi.setPath("checks.filter", "all");
  }
  go({ tab: "Checks" });
  reveal(rowId.check(row.spot.hash));
}

/** Every check of the installed seed, by its place and the user's label. */
export function checks(rows: Row[]): Hit[] {
  return rows.map((r) =>
    hit({
      group: "checks",
      key: r.spot.hash,
      trail: ["Checks", areaOf(r)],
      title: placeOf(r),
      icon: ListChecks,
      body: r.label ? `your label: ${r.label}` : undefined,
      run: () => openCheck(rows, r),
    }),
  );
}

/** What the actions read: each one's block, or "" when it can run. */
export interface ActionState {
  /** Why nothing can be installed. */
  blocked: string;
  /** Why nothing can be built. */
  unbuildable: string;
  generating: boolean;
  /** Another action is under way. */
  busy: boolean;
  /** The target Play opens, if any. */
  picked?: string;
  /** Why Play can't start it. */
  cantPlay: string;
  /** Its game lacks the installed hook patch: Play asks on its page. */
  stale: boolean;
  theme: string;
  generate: (install: boolean) => void;
  stop: () => void;
  play: () => Promise<string | undefined>;
  nextTheme: () => void;
  checkNow: () => Promise<string>;
}

/** The app's actions, each what its button does, under its button's
 * blocks. */
export function actions(s: ActionState): Hit[] {
  const toPlay = () => go({ tab: "Play", folder: "standard" });
  const waiting = s.generating
    ? "A seed is generating"
    : s.busy && "Waiting for the last action to finish";
  return [
    hit({
      group: "actions",
      key: "install",
      trail: ["Action"],
      title: "Generate & install",
      icon: Sparkles,
      body: "Builds a seed and installs it to every target",
      why: waiting || s.blocked,
      run: () => {
        s.generate(true);
        toPlay();
      },
    }),
    hit({
      group: "actions",
      key: "generate",
      trail: ["Action"],
      title: "Generate only",
      icon: ListPlus,
      body: "Adds a seed to History without installing",
      why: waiting || s.unbuildable,
      run: () => {
        s.generate(false);
        toPlay();
      },
    }),
    ...(s.generating
      ? [
          hit({
            group: "actions",
            key: "stop",
            trail: ["Action"],
            title: "Stop generating",
            icon: CircleStop,
            body: "Stops the seed being built",
            run: s.stop,
          }),
        ]
      : []),
    hit({
      group: "actions",
      key: "play",
      trail: ["Action"],
      title: s.picked ? `Play ${s.picked}` : "Play",
      icon: Play,
      body: s.stale
        ? "Its game needs a rebuild first: opens Play to ask"
        : "Starts the game",
      why: s.cantPlay,
      pending: "Starting…",
      run: s.stale ? toPlay : s.play,
    }),
    hit({
      group: "actions",
      key: "theme",
      trail: ["Action"],
      title: "Toggle theme",
      icon: SunMoon,
      body: `Theme: ${s.theme}`,
      run: s.nextTheme,
    }),
    hit({
      group: "actions",
      key: "update",
      trail: ["Action"],
      title: "Check for updates",
      icon: RefreshCw,
      body: "Asks GitHub for the latest release now",
      pending: "Checking…",
      run: s.checkNow,
    }),
  ];
}

/** Every word a hit matches on. */
const text = (h: Hit) =>
  [...(h.trail ?? []), h.title, h.body ?? "", h.words ?? ""].join(" ");

/** The hits holding every word of `query`. In each group, those whose
 * title holds them all come first. */
export function search(hits: Hit[], query: string): Hit[] {
  const ws = words(query);
  if (ws.length === 0) return [];
  const found = hits.filter((h) => hasAll(text(h), ws));
  const titled = (h: Hit) => (hasAll(h.title, ws) ? 0 : 1);
  return found.sort((a, b) => titled(a) - titled(b));
}

/** The recent picks still there, newest first, as hits of the "recent"
 * group. */
export function recents(hits: Hit[], recent: Found[]): Hit[] {
  return recent.flatMap(({ group, key }) => {
    const h = hits.find((x) => x.found.group === group && x.found.key === key);
    return h ? [{ ...h, id: `recent:${h.id}`, group: "recent" as const }] : [];
  });
}
