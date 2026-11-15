import React, { useEffect, useState } from "react";
import { commands, type DevActor } from "@/bindings";
import { call, message } from "@/lib/api";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

interface Base {
  /** The map they were read for. */
  of: string | null;
}

/** A map's actors once read, why they couldn't be, or still reading. */
type Actors = (
  | { status: "ok"; list: DevActor[] }
  | { status: "error"; error: string }
  | { status: "loading" }
) &
  Base;

/** Before any map's actors are read. */
const DEFAULT_ACTORS: Actors = {
  status: "loading",
  of: null,
};

/** The current map's actors matching Teleport's search, read again when the
 * map changes, or why they couldn't be. */
export function useActors(): Actors {
  const [map] = useDev.usePath("game.state.map");
  const [query] = useUi.usePath("dev.teleport.actors");
  const [actors, setActors] = useState<Actors>(DEFAULT_ACTORS);

  useEffect(() => {
    if (!map) return;

    let live = true;
    setActors({ of: map, status: "loading" });
    call(commands.devActors(map))
      .then(
        (found) => live && setActors({ of: map, list: found, status: "ok" }),
      )
      .catch(
        (e) =>
          live && setActors({ of: map, error: message(e), status: "error" }),
      );
    return () => {
      live = false;
    };
  }, [map]);

  return React.useMemo(() => {
    const q = query.toLowerCase().trim();
    if (actors.of !== map) return DEFAULT_ACTORS;
    if (actors.status === "ok")
      return {
        ...actors,
        list: actors.list.filter((p) =>
          [p.actor, p.label ?? ""].some((t) => t.toLowerCase().includes(q)),
        ),
      };
    return actors;
  }, [query, actors, map]);
}
