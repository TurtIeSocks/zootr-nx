import { commands } from "@/bindings";
import { Input } from "@/components/ui/input";
import { areas, type Spot } from "@/dev/areas";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Arrival points for Link's age, by area, a click teleporting there. */
export function Places() {
  const [places] = useDev.usePath("options.places");
  const [child] = useDev.usePath("game.state.child");
  const [query, setQuery] = useUi.usePath("dev.teleport.places");
  const shown = areas(places, child, query);

  return (
    <>
      <Input
        aria-label="Search places"
        placeholder="Area or entrance"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <div className="mt-1.5 max-h-72 overflow-auto rounded-md border border-line">
        {shown.map(({ area, spots }) => (
          <div key={area}>
            <h3 className="sticky top-0 bg-surface-raised px-2 py-1 font-medium text-xs">
              {area}
            </h3>
            <ul aria-label={area}>
              {spots.map((s) => (
                <li key={s.place}>
                  <PlaceButton spot={s} />
                </li>
              ))}
            </ul>
          </div>
        ))}
        {shown.length === 0 && (
          <p className="px-2 py-1 text-ink-faint text-xs">No places match</p>
        )}
      </div>
    </>
  );
}

/** Teleports Link to `spot`, showing only its spot, since the area heads
 * the list. */
function PlaceButton({ spot }: { spot: Spot }) {
  const [ready] = useDev.usePath("ready");
  const request = useRequest();

  return (
    <button
      type="button"
      disabled={!ready || request.busy}
      onClick={() => request.run(() => call(commands.devTeleport(spot.place)))}
      className="block w-full cursor-pointer truncate px-3 py-0.5 text-left text-[13px] enabled:hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-45"
    >
      {spot.spot}
    </button>
  );
}
