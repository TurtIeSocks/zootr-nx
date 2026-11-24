import { Search } from "lucide-react";
import type { PropsWithChildren } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { warpBeside } from "@/dev/requests";
import { LIST } from "@/dev/styles";
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
        <ActorRows />
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
function ActorRows() {
  const actors = useActors();

  if (actors.status !== "ok")
    return (
      <li className="px-3 py-1 text-ink-faint text-xs">
        {actors.status === "error" ? actors.error : "Reading the map…"}
      </li>
    );

  if (actors.rows.length === 0)
    return (
      <li className="px-3 py-1 text-ink-faint text-xs">No actors match</li>
    );

  return actors.rows.map((row) => (
    <li key={row.placed.hash}>
      <ActorButton row={row} />
    </li>
  ));
}

/** Warps Link beside the row's placement on the picked side, its tag
 * telling it from its actor's other placements. */
function ActorButton({ row: { placed, tag } }: { row: ActorRow }) {
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
