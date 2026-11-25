import { Search, Star } from "lucide-react";
import type { PropsWithChildren } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { GroupHead } from "@/dev/dev-card";
import { warpBeside } from "@/dev/requests";
import { distance, sortRows, useLinkPos } from "@/dev/sort";
import { SortToggle } from "@/dev/sort-toggle";
import { StarButton } from "@/dev/star-button";
import { useStars } from "@/dev/stars";
import { LIST, ROW, ROW_BUTTON } from "@/dev/styles";
import { type ActorRow, useActors } from "@/dev/use-actors";
import { SIDES } from "@/lib/sides";
import { cn } from "@/lib/utils";
import { useCanSend, useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** A search field with its icon. */
export function SearchField({
  label,
  placeholder,
  value,
  onChange,
  className,
}: {
  label: string;
  placeholder: string;
  value: string;
  onChange: (v: string) => void;
  className?: string;
}) {
  return (
    <div className={cn("relative min-w-0", className)}>
      <Search
        aria-hidden
        className="pointer-events-none absolute top-2 left-2.5 size-4 text-ink-faint"
      />
      <Input
        aria-label={label}
        placeholder={placeholder}
        className="pl-8"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

/** Its children, once the game says which map Link is on. */
export function MapGate({ children }: PropsWithChildren) {
  const [map] = useDev.usePath("game.state.map");

  if (!map) {
    return <p className="text-ink-faint text-xs">Waiting for the map</p>;
  }
  return children;
}

/** The map's placements, searched and sorted, the starred ones on top, a
 * click warping Link beside one on the side picked in Warp. Warp's list
 * (`list="warp"`) also picks that side. */
export function MapActors({ list }: { list: "warp" | "all" }) {
  const [query, setQuery] = useUi.usePath("dev.teleport.actors");
  const [map] = useDev.usePath("game.state.map");

  return (
    <>
      <div className="flex items-center gap-2">
        <SearchField
          label="Search actors"
          placeholder={`Actors in ${map}`}
          value={query}
          onChange={setQuery}
          className="flex-1"
        />
        <SortToggle list={list} />
      </div>
      {list === "warp" && <TeleportSides />}
      <ActorLists list={list} map={map} />
    </>
  );
}

/** The side of the actor Link warps to. */
function TeleportSides() {
  const [side, setSide] = useUi.usePath("dev.teleport.side");

  return (
    <div className="flex items-center gap-2">
      <Label id="warp-side" className="text-xs">
        Side
      </Label>
      <Segmented
        aria-labelledby="warp-side"
        options={SIDES}
        value={side}
        onValueChange={(id) => setSide(id)}
      />
    </div>
  );
}

/** A row as the list shows it: the placement, its tag, and how far it's
 * placed from Link when that's known and the list sorts by it. */
interface Shown extends ActorRow {
  distance?: number;
}

/** The starred placements of this map, then every one matching the
 * search, in the list's order. Or why there are none to show. */
function ActorLists({ list, map }: { list: "warp" | "all"; map: string }) {
  const actors = useActors();
  const [sort] = useUi.usePath(`dev.sort.${list}`);
  const link = useLinkPos(sort === "near");
  const stars = useStars("actors");

  const note = (text: string) => (
    <li className="px-3 py-1 text-ink-faint text-xs">{text}</li>
  );
  const rows = sortRows(
    (actors.status === "ok" ? actors.rows : []).map(
      (r): Shown => ({
        ...r,
        distance: sort === "near" ? distance(link, r.placed.pos) : undefined,
      }),
    ),
    sort,
    (r) => ({ name: r.placed.actor, tag: r.tag, distance: r.distance }),
  );
  const starred = rows.filter((r) => stars.has(r.placed.hash));
  const rowsOf = (shown: Shown[]) =>
    shown.map((row) => (
      <li key={row.placed.hash}>
        <ActorRowView row={row} />
      </li>
    ));

  return (
    <div className={cn(LIST, "min-h-0 flex-1 pb-1")}>
      {starred.length > 0 && (
        <>
          <GroupHead title="Starred" count={starred.length} />
          <ul aria-label="Starred actors">{rowsOf(starred)}</ul>
        </>
      )}
      <ul aria-label={`Actors in ${map}`} className="pt-1">
        {actors.status === "error"
          ? note(actors.error)
          : actors.status === "loading"
            ? note("Reading the map…")
            : rows.length === 0
              ? note("No actors match")
              : rowsOf(rows)}
      </ul>
    </div>
  );
}

/** A placement: a click warps Link beside it on the side picked in Warp,
 * its tag telling it from its actor's other placements. Its star, shown on
 * hover or once starred, stars it. */
function ActorRowView({ row: { placed, tag, distance } }: { row: Shown }) {
  const can = useCanSend();
  const stars = useStars("actors");
  const on = stars.has(placed.hash);
  const name = tag ? `${placed.actor} ${tag}` : placed.actor;

  return (
    <div className={cn(ROW, "group")}>
      <button
        type="button"
        disabled={!can}
        onClick={() => void warpBeside(placed.actor, placed.hash)}
        className={ROW_BUTTON}
      >
        {on && <Star className="fill-gold text-gold" />}
        <span className="truncate">{placed.actor}</span>
        {tag && <span className="truncate text-ink-faint">{tag}</span>}
        {distance !== undefined && (
          <span className="ml-auto shrink-0 text-ink-faint tabular-nums">
            {distance.toFixed(1)} m
          </span>
        )}
      </button>
      <StarButton
        name={name}
        on={on}
        onToggle={() => stars.toggle(placed.hash)}
      />
    </div>
  );
}
