import {
  commands,
  type PaletteView,
  type PlandoOptions,
  type TrackView,
} from "@/bindings";
import { call } from "@/lib/api";

/** Each palette catalog read, by {@link paletteKey}. Reading checks every
 * palette against the dump, which takes a while, so it's read once and
 * kept until the app quits. */
export const palettes = new Map<string, PaletteView[]>();

/** The palettes' key: the dump and game assets setting they're read with. */
export const paletteKey = (game: string | null, art: unknown) =>
  `${game}\n${art}`;

/** The palettes for `key`, read once. Kept even when no one waits on it
 * any more. */
export async function readPalettes(key: string): Promise<PaletteView[]> {
  const kept = palettes.get(key);
  if (kept) return kept;
  const read = await call(commands.colorsCatalog());
  palettes.set(key, read);
  return read;
}

/** The tracks, read once: they're the game's, the same in every dump. */
let tracks: Promise<TrackView[]> | undefined;

export function readTracks(): Promise<TrackView[]> {
  tracks ??= commands.audioCatalog().catch((e) => {
    tracks = undefined;
    throw e;
  });
  return tracks;
}

/** The checks and items a plando can pin, with the current settings. */
export const readPlando = (): Promise<PlandoOptions> =>
  call(commands.plandoOptions());
