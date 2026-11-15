import { commands, type DevActor } from "@/bindings";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { useActors } from "@/dev/use-actors";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { SIDES } from "@/lib/sides";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** The current map's actors, a click warping Link to the picked side. */
export function Actors() {
  return (
    <MapGate>
      <ActorsQuery />
      <TeleportSides />
      <ActorsList>
        <ActorsStatusGate />
      </ActorsList>
    </MapGate>
  );
}

/** The side of the actor Link warps to. */
function TeleportSides() {
  const [side, setSide] = useUi.usePath("dev.teleport.side");

  return (
    <div className="mt-1.5 flex items-center gap-2">
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

/** Its children, once the game says which map Link is on. */
function MapGate({ children }: React.PropsWithChildren) {
  const [map] = useDev.usePath("game.state.map");

  if (!map) {
    return <p className="text-ink-faint text-xs">Waiting for the map</p>;
  }
  return children;
}

/** The search over the map's actors. */
function ActorsQuery() {
  const [query, setQuery] = useUi.usePath("dev.teleport.actors");
  const [map] = useDev.usePath("game.state.map");

  return (
    <Input
      aria-label="Search actors"
      placeholder={`Actors in ${map}`}
      value={query}
      onChange={(e) => setQuery(e.target.value)}
    />
  );
}

/** The list the map's actors go in. */
function ActorsList({ children }: React.PropsWithChildren) {
  const [map] = useDev.usePath("game.state.map");

  return (
    <ul
      aria-label={`Actors in ${map}`}
      className="mt-1.5 max-h-72 overflow-auto rounded-md border border-line"
    >
      {children}
    </ul>
  );
}

/** The actors matching the search, or why there are none to show. */
function ActorsStatusGate() {
  const actors = useActors();

  if (actors.status !== "ok")
    return (
      <li className="px-2 py-1 text-ink-faint text-xs">
        {actors.status === "error" ? actors.error : "Reading the map…"}
      </li>
    );

  if (actors.list.length === 0)
    return (
      <li className="px-2 py-1 text-ink-faint text-xs">No actors match</li>
    );

  return actors.list.map((placed) => (
    <li key={placed.hash}>
      <ActorButton placed={placed} tag={tag(actors.list, placed)} />
    </li>
  ));
}

/** What tells `placed` from its actor's other placements in `list`: its own
 * name, else its place among them. None when the actor is placed once. */
function tag(list: DevActor[], placed: DevActor): string | null {
  const same = list.filter((p) => p.actor === placed.actor);
  if (same.length === 1) return null;
  return placed.label ?? `#${same.indexOf(placed) + 1}`;
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
  const request = useRequest();
  const [ready] = useDev.usePath("ready");

  return (
    <button
      type="button"
      disabled={!ready || request.busy}
      onClick={() =>
        request.run(() =>
          call(
            commands.devWarp(placed.hash, useUi.getPath("dev.teleport.side")),
          ),
        )
      }
      className="flex w-full cursor-pointer justify-between gap-2 px-3 py-0.5 text-left text-[13px] enabled:hover:bg-surface-hover disabled:opacity-45 disabled:cursor-not-allowed"
    >
      <span className="truncate">{placed.actor}</span>
      {tag && <span className="truncate text-ink-faint">{tag}</span>}
    </button>
  );
}
