import { CornerDownRight, History, Star } from "lucide-react";
import type { ReactNode } from "react";
import { Segmented } from "@/components/ui/segmented";
import { area, areas, type Spot, spots } from "@/dev/areas";
import { GroupHead } from "@/dev/dev-card";
import { MapActors, MapGate, SearchField } from "@/dev/map-actors";
import { RECENT_PLACES, warpTo } from "@/dev/requests";
import { StarButton } from "@/dev/star-button";
import { toggled } from "@/dev/stars";
import { LIST, ROW, ROW_BUTTON } from "@/dev/styles";
import { cn } from "@/lib/utils";
import { useCanSend, useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Warp's tabs. */
const TABS: { id: "places" | "actors"; label: string }[] = [
  { id: "places", label: "Places" },
  { id: "actors", label: "Actors in Map" },
];

/** Teleport to an arrival point, or warp beside an actor of the map. */
export function WarpTab() {
  const [tab, setTab] = useUi.usePath("dev.teleport.tab");
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2.5">
      <Segmented
        aria-label="Warp to"
        full
        options={TABS}
        value={tab}
        onValueChange={setTab}
      />
      {tab === "places" ? (
        <Places />
      ) : (
        <MapGate>
          <MapActors list="warp" />
        </MapGate>
      )}
    </div>
  );
}

/** Starred and recent places, then every arrival point for Link's age by
 * area, the area Link is in first, a click teleporting there. */
function Places() {
  const [places] = useDev.usePath("options.places");
  const [child] = useDev.usePath("game.state.child");
  const [map] = useDev.usePath("game.state.map");
  const [starred] = useUi.usePath("dev.starred");
  const [recent] = useUi.usePath("dev.recent.places");
  const [query, setQuery] = useUi.usePath("dev.teleport.places");
  const shown = areas(places, child, query, map);
  const stars = spots(starred, child, query);
  // Starred places leave Recent, which still shows its five.
  const lately = spots(
    recent.filter((p) => !starred.includes(p)),
    child,
    query,
  ).slice(0, RECENT_PLACES);

  return (
    <>
      <SearchField
        label="Search places"
        placeholder="Area or entrance"
        value={query}
        onChange={setQuery}
      />
      <div className={cn(LIST, "min-h-0 flex-1 pb-1")}>
        {stars.length > 0 && (
          <PlaceGroup title="Starred" spots={stars} kind="starred" />
        )}
        {lately.length > 0 && (
          <PlaceGroup title="Recent" spots={lately} kind="recent" />
        )}
        {shown.map((a) => (
          <PlaceGroup key={a.area} title={a.area} spots={a.spots} kind="area" />
        ))}
        {shown.length === 0 && (
          <p className="px-3 py-2 text-ink-faint text-xs">No places match</p>
        )}
      </div>
    </>
  );
}

/** A group of places under its heading. Starred and Recent show each
 * one's area too, since they mix areas. */
function PlaceGroup({
  title,
  spots,
  kind,
}: {
  title: string;
  spots: Spot[];
  kind: "starred" | "recent" | "area";
}) {
  const one = kind === "area";
  return (
    <div>
      <GroupHead title={title} count={spots.length} sticky={one} />
      <ul aria-label={title}>
        {spots.map((s) => (
          <li key={s.place}>
            <PlaceRow spot={s} mixed={!one}>
              {kind === "recent" ? <History /> : one && <CornerDownRight />}
            </PlaceRow>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** A place: a click teleports there; its star, shown on hover or once
 * starred, stars it. */
function PlaceRow({
  spot,
  mixed,
  children,
}: {
  spot: Spot;
  mixed: boolean;
  children: ReactNode;
}) {
  const can = useCanSend();
  const [starred, setStarred] = useUi.usePath("dev.starred");
  const on = starred.includes(spot.place);

  return (
    <div className={cn(ROW, "group")}>
      <button
        type="button"
        disabled={!can}
        onClick={() => void warpTo(spot)}
        className={ROW_BUTTON}
      >
        {on ? <Star className="fill-gold text-gold" /> : children}
        <span className="truncate">{spot.spot}</span>
        {mixed && (
          <span className="truncate text-ink-faint">{area(spot.map)}</span>
        )}
      </button>
      <StarButton
        name={spot.spot}
        on={on}
        onToggle={() => setStarred((s) => toggled(s, spot.place))}
      />
    </div>
  );
}
