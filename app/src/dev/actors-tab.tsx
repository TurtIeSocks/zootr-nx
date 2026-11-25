import { Eye, EyeOff, Navigation, RefreshCw, X } from "lucide-react";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { Switch } from "@/components/ui/switch";
import { Ago } from "@/dev/activity";
import { DevCard, GroupHead } from "@/dev/dev-card";
import { MapActors, MapGate, SearchField } from "@/dev/map-actors";
import { warpBeside } from "@/dev/requests";
import { sortRows } from "@/dev/sort";
import { SortToggle } from "@/dev/sort-toggle";
import { StarButton } from "@/dev/star-button";
import { useStars } from "@/dev/stars";
import { HOVER_ACTION, LIST, ROW, ROW_BUTTON } from "@/dev/styles";
import {
  type HiddenChip,
  type LiveRow,
  named,
  readLive,
  showAll,
  showOne,
  toggle,
  useHiddenChips,
  useLiveReads,
  useLiveRows,
} from "@/dev/use-live-actors";
import { cn } from "@/lib/utils";
import { send, useCanSend, useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Actors' tabs. */
const TABS: { id: "nearby" | "all"; label: string }[] = [
  { id: "nearby", label: "Nearby" },
  { id: "all", label: "All in Map" },
];

/** The live actors nearest Link, each one's model hidden or shown with a
 * click, or every placement of the map, each a warp away. */
export function ActorsTab() {
  const [tab, setTab] = useUi.usePath("dev.actors.tab");
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5">
      <Segmented
        aria-label="Actors"
        full
        options={TABS}
        value={tab}
        onValueChange={setTab}
      />
      <MapGate>
        {tab === "nearby" ? <Nearby /> : <MapActors list="all" />}
      </MapGate>
    </div>
  );
}

/** Keeps the live list read, and the hidden actors hidden after each
 * load, whichever tab is open. Shows nothing. */
export function LiveReads() {
  useLiveReads();
  return null;
}

function Nearby() {
  return (
    <>
      <HiddenCard />
      <LiveControls />
      <LiveList />
    </>
  );
}

/** The actors hidden so far, each shown again with its ×, or all at once. */
function HiddenCard() {
  const chips = useHiddenChips();
  const can = useCanSend();

  if (chips.length === 0) return null;
  return (
    <DevCard title="Hidden" meta="hidden again after each load">
      <ul aria-label="Hidden actors" className="flex flex-wrap gap-1.5">
        {chips.map((chip) => (
          <li key={chip.hidden.hash}>
            <Chip chip={chip} />
          </li>
        ))}
      </ul>
      <div className="mt-1.5 flex justify-end">
        <Button
          size="sm"
          variant="ghost"
          disabled={!can}
          onClick={() => send("hide", "show every hidden actor", showAll)}
        >
          Show All
        </Button>
      </div>
    </DevCard>
  );
}

/** One hidden actor, dimmed while it isn't hidden in the game now. */
function Chip({ chip: { hidden, tag, live } }: { chip: HiddenChip }) {
  const can = useCanSend();
  const name = named(hidden.actor, tag);

  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 rounded-sm bg-surface-sunken py-0.5 pl-2 text-[12px]",
        !live && "opacity-50",
      )}
    >
      {hidden.actor}
      {tag && <span className="text-ink-faint">{tag}</span>}
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={`Show ${name}`}
        disabled={!can}
        onClick={() => send("hide", `show ${name}`, () => showOne(hidden, tag))}
      >
        <X />
      </Button>
    </span>
  );
}

/** The search and Follow. */
function LiveControls() {
  const [query, setQuery] = useDev.usePath("live.query");
  const [follow, setFollow] = useDev.usePath("live.follow");

  return (
    <div className="flex items-center gap-2">
      <SearchField
        label="Search live actors"
        placeholder="Search nearby actors"
        value={query}
        onChange={setQuery}
        className="flex-1"
      />
      <SortToggle list="nearby" />
      <Switch id="live-follow" checked={follow} onCheckedChange={setFollow} />
      <Label htmlFor="live-follow" className="text-xs">
        Follow
      </Label>
    </div>
  );
}

/** The starred live actors, then every one matching the search, in the
 * picked order at the last read. Or why there are none to show. */
function LiveList() {
  const live = useLiveRows();
  const [map] = useDev.usePath("game.state.map");
  const [ready] = useDev.usePath("ready");
  const [sort] = useUi.usePath("dev.sort.nearby");
  const stars = useStars("actors");
  const rows =
    live.status === "ok"
      ? sortRows(live.rows, sort, (r) => ({
          name: r.near.actor,
          tag: r.tag,
          distance: r.near.distance ?? undefined,
        }))
      : [];
  const starred = rows.filter((r) => stars.has(r.near.hash));

  return (
    <div className={cn(LIST, "min-h-0 flex-1 pb-1")}>
      <div className="sticky top-0 z-1 flex items-center bg-surface pr-1">
        <div className="min-w-0 flex-1">
          <GroupHead
            title={sort === "near" ? "Nearest First" : "A–Z"}
            count={
              live.status === "ok" ? (
                <span className="normal-case tracking-normal">
                  read <Ago at={live.at} />
                </span>
              ) : undefined
            }
          />
        </div>
        <Button
          size="icon-xs"
          variant="ghost"
          aria-label="Refresh"
          disabled={!ready}
          onClick={() => void readLive()}
        >
          <RefreshCw />
        </Button>
      </div>
      {live.status === "ok" && starred.length > 0 && (
        <>
          <GroupHead title="Starred" count={starred.length} />
          <ul aria-label="Starred live actors">
            <LiveRows rows={starred} map={live.map} />
          </ul>
        </>
      )}
      <ul aria-label={`Live actors in ${map}`}>
        {live.status === "ok" ? (
          <LiveRows rows={rows} map={live.map} />
        ) : (
          <li className="px-3 py-1 text-ink-faint text-xs">
            {live.status === "error" ? live.error : "Reading the actors…"}
          </li>
        )}
      </ul>
    </div>
  );
}

function LiveRows({ rows, map }: { rows: LiveRow[]; map: string }) {
  if (rows.length === 0)
    return (
      <li className="px-3 py-1 text-ink-faint text-xs">No actors match</li>
    );
  return rows.map((row) => (
    <li key={row.near.hash}>
      {row.near.hidden === "game" ? (
        <GameHidden row={row} />
      ) : (
        <LiveActor row={row} map={map} />
      )}
    </li>
  ));
}

/** A row's name, tag and distance, after its leading mark. */
function RowText({
  row: { near, tag },
  note,
  struck,
}: {
  row: LiveRow;
  note?: string;
  struck?: boolean;
}) {
  return (
    <>
      <span className={cn("truncate", struck && "line-through")}>
        {near.actor}
      </span>
      {tag && (
        <span
          className={cn("truncate text-ink-faint", struck && "line-through")}
        >
          {tag}
        </span>
      )}
      <span className="ml-auto shrink-0 text-ink-faint tabular-nums">
        {note ?? `${near.distance?.toFixed(1) ?? "?"} m`}
      </span>
    </>
  );
}

/** A live actor: a click hides its model or shows it again, and Warp
 * beside, on hover, takes Link to it on the side picked in Warp. */
function LiveActor({ row, map }: { row: LiveRow; map: string }) {
  const can = useCanSend();
  const hidden = row.near.hidden === "ours";
  const { actor, hash } = row.near;
  const name = named(actor, row.tag);
  const stars = useStars("actors");

  return (
    <div className={cn(ROW, "group")}>
      <button
        type="button"
        aria-pressed={hidden}
        disabled={!can}
        onClick={() =>
          send("hide", `${hidden ? "show" : "hide"} ${name}`, () =>
            toggle(row.near, map, row.tag),
          )
        }
        className={cn(ROW_BUTTON, hidden && "text-ink-muted")}
      >
        {hidden ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
        <RowText row={row} struck={hidden} />
      </button>
      <Tip tip="Warp beside">
        <button
          type="button"
          aria-label={`Warp beside ${name}`}
          disabled={!can}
          onClick={() => void warpBeside(actor, hash)}
          className={cn(HOVER_ACTION, "text-ink-muted")}
        >
          <Navigation />
        </button>
      </Tip>
      <StarButton
        name={name}
        on={stars.has(hash)}
        onToggle={() => stars.toggle(hash)}
      />
    </div>
  );
}

/** An actor the game keeps hidden: shown, not toggled, since a show never
 * undoes the game's own hide. */
function GameHidden({ row }: { row: LiveRow }) {
  return (
    <div className={cn(ROW, "py-1 pl-3 text-ink-faint")}>
      <EyeOff aria-hidden className="mr-1 opacity-50" />
      <RowText row={row} note="hidden by game" />
    </div>
  );
}
