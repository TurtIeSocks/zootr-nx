import { commands } from "@/bindings";
import type { Spot } from "@/dev/areas";
import { call } from "@/lib/api";
import { send, useDev } from "@/stores/dev";
import { type DevGive, remember, useUi } from "@/stores/ui";

/** How many places Warp's Recent shows. */
export const RECENT_PLACES = 5;
/** How many places Warp's Recent keeps: more than it shows, so it still
 * shows five once starred ones leave it. */
export const RECENT_PLACES_KEPT = 20;
/** How many gives Give's recents keep. */
export const RECENT_GIVES = 3;
/** How many names Values' Recent keeps. */
export const RECENT_VALUES = 5;

/** A give as its repeat button says it: the item, and the count past
 * one. */
export const giveLabel = ({ item, count }: DevGive) =>
  count === 1 ? item : `${item} ×${count}`;

/** Teleports Link to `spot`. One that works heads Warp's Recent and names
 * the header's place until Link leaves the map. */
export const warpTo = (spot: Spot) =>
  send("warp", `warp to ${spot.spot}`, async () => {
    const said = await call(commands.devTeleport(spot.place));
    useDev.setPath("lastWarp", { map: spot.map, spot: spot.spot });
    useUi.setPath("dev.recent.places", (r) =>
      remember(r, spot.place, RECENT_PLACES_KEPT),
    );
    return said;
  });

/** Warps Link beside a placement, on the side picked in Warp. */
export const warpBeside = (actor: string, hash: string) =>
  send("warp", `warp beside ${actor}`, () =>
    call(commands.devWarp(hash, useUi.getPath("dev.teleport.side"))),
  );

/** Gives Link `count` of `item`. One that works heads Give's recents. */
export const give = (item: string, count: number) =>
  send("give", `give ${item}`, async () => {
    const said = await call(commands.devGive(item, count));
    useUi.setPath("dev.recent.gives", (r) =>
      remember(
        r,
        { item, count },
        RECENT_GIVES,
        (a, b) => a.item === b.item && a.count === b.count,
      ),
    );
    return said;
  });

/** Applies a saved test setup now, keeping which of its goals held
 * before. Its entry has a line per thing it did. */
export const apply = (name: string) =>
  send("apply", `apply ${name}`, async () => {
    const applied = await call(commands.devApply(name));
    useDev.setPath("tests.holds", (holds) => ({
      ...holds,
      [name]: applied.held,
    }));
    const early = applied.held.map((g) => `${g} held before Apply`);
    return [`Applied ${name}`, ...applied.done, ...early].join("\n");
  });

/** Starts `target`, unless its build lacks the installed hook patch: the
 * main window's Play asks about a rebuild then, so it's played there. */
export const play = (target: string) =>
  send("play", `play ${target}`, async () => {
    const status = await call(commands.status());
    const kind = status.targets.find((t) => t.name === target)?.build?.status
      .kind;
    if (kind === "stale" || kind === "refused")
      throw new Error(
        "its build lacks the installed hook patch. Play it from the main window, which can rebuild it",
      );
    await call(commands.play(target));
    return `Started ${target}`;
  });
