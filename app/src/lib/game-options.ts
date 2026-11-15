import { commands, type GameOptions } from "@/bindings";
import { call } from "@/lib/api";
import { type Loaded, useLoad } from "@/lib/use-load";
import { useConfig } from "@/stores/config";

/** Each dump's options: the scan takes a few seconds. */
const scans = new Map<string, Promise<GameOptions>>();

/** What a game dump offers to pick from, scanned once a dump. A failed scan
 * is forgotten, so the next one tries again. */
export function scanGame(game: string): Promise<GameOptions> {
  let found = scans.get(game);
  if (!found) {
    found = call(commands.gameOptions());
    found.catch(() => scans.delete(game));
    scans.set(game, found);
  }
  return found;
}

/** The saved game dump's options, scanned once. */
export function useGameOptions(): Loaded<GameOptions> {
  const [game] = useConfig.usePath("config.game");
  return useLoad(scanGame, game ?? "");
}

/** The options that are plain lists of names. */
type List = Exclude<keyof GameOptions, "quests">;

/** A list of the options once in, else empty, so a picker takes any text. */
export const listed = (options: Loaded<GameOptions>, list: List): string[] =>
  options.status === "ok" ? options.data[list] : [];

/** Whether `value` is one of the dump's `list`. Anything is, until the
 * scan answers. */
export const known = (
  options: Loaded<GameOptions>,
  list: List,
  value: string,
) => options.status !== "ok" || options.data[list].includes(value);
