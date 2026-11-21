import { CornerDownRight, History, Search, Star } from "lucide-react";
import type { ReactNode } from "react";
import type { DevActor } from "@/bindings";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { area, areas, type Spot, spots } from "@/dev/areas";
import { GroupHead } from "@/dev/dev-card";
import { warpBeside, warpTo } from "@/dev/requests";
import { HOVER_ACTION, LIST, ROW, ROW_BUTTON } from "@/dev/styles";
import { tag } from "@/dev/tag";
import { useActors } from "@/dev/use-actors";
import { SIDES } from "@/lib/sides";
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
          <MapActors sides />
        </MapGate>
      )}
    </div>
  );
}

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

/** Starred and recent places, then every arrival point for Link's age by
 * area, a click teleporting there. */
function Places() {
  const [places] = useDev.usePath("options.places");
  const [child] = useDev.usePath("game.state.child");
  const [starred] = useUi.usePath("dev.starred");
  const [recent] = useUi.usePath("dev.recent.places");
  const [query, setQuery] = useUi.usePath("dev.teleport.places");
  const shown = areas(places, child, query);
  const stars = spots(starred, child, query);
  const lately = spots(recent, child, query);

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
          <PlaceGroup title="Starred" spots={stars} icon="star" />
        )}
        {lately.length > 0 && (
          <PlaceGroup title="Recent" spots={lately} icon="recent" />
        )}
        {shown.map(({ area, spots }) => (
          <PlaceGroup key={area} title={area} spots={spots} />
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
  icon,
}: {
  title: string;
  spots: Spot[];
  icon?: "star" | "recent";
}) {
  return (
    <div>
      <GroupHead title={title} count={spots.length} sticky={!icon} />
      <ul aria-label={title}>
        {spots.map((s) => (
          <li key={s.place}>
            <PlaceRow spot={s} mixed={!!icon}>
              {icon === "recent" ? <History /> : !icon && <CornerDownRight />}
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
      <button
        type="button"
        aria-label={`${on ? "Unstar" : "Star"} ${spot.spot}`}
        aria-pressed={on}
        onClick={() =>
          setStarred((s) =>
            on ? s.filter((p) => p !== spot.place) : [...s, spot.place],
          )
        }
        className={cn(
          HOVER_ACTION,
          on ? "text-gold opacity-100 [&_svg]:fill-gold" : "text-ink-faint",
        )}
      >
        <Star />
      </button>
    </div>
  );
}

/** Its children, once the game says which map Link is on. */
export function MapGate({ children }: React.PropsWithChildren) {
  const [map] = useDev.usePath("game.state.map");

  if (!map) {
    return <p className="text-ink-faint text-xs">Waiting for the map</p>;
  }
  return children;
}

/** The map's placements, searched, a click warping Link beside one on the
 * side picked here (with `sides`) or in Warp. */
export function MapActors({ sides }: { sides?: boolean }) {
  const [query, setQuery] = useUi.usePath("dev.teleport.actors");
  const [map] = useDev.usePath("game.state.map");

  return (
    <>
      <SearchField
        label="Search actors"
        placeholder={`Actors in ${map}`}
        value={query}
        onChange={setQuery}
      />
      {sides && <TeleportSides />}
      <ul
        aria-label={`Actors in ${map}`}
        className={cn(LIST, "min-h-0 flex-1 py-1")}
      >
        <ActorsStatusGate />
      </ul>
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

/** The actors matching the search, or why there are none to show. */
function ActorsStatusGate() {
  const actors = useActors();

  if (actors.status !== "ok")
    return (
      <li className="px-3 py-1 text-ink-faint text-xs">
        {actors.status === "error" ? actors.error : "Reading the map…"}
      </li>
    );

  if (actors.list.length === 0)
    return (
      <li className="px-3 py-1 text-ink-faint text-xs">No actors match</li>
    );

  return actors.list.map((placed) => (
    <li key={placed.hash}>
      <ActorButton placed={placed} tag={tag(actors.list, placed)} />
    </li>
  ));
}

/** Warps Link beside `placed` on the picked side, `tag` telling it from
 * its actor's other placements. */
function ActorButton({
  placed,
  tag,
}: {
  placed: DevActor;
  tag: string | null;
}) {
  const can = useCanSend();

  return (
    <button
      type="button"
      disabled={!can}
      onClick={() => void warpBeside(placed.actor, placed.hash)}
      className="flex w-full cursor-pointer justify-between gap-2 px-3 py-1 text-left text-[13px] enabled:hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-45"
    >
      <span className="truncate">{placed.actor}</span>
      {tag && <span className="truncate text-ink-faint">{tag}</span>}
    </button>
  );
}
