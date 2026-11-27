import type { Favorite, Label } from "@/bindings";
import type { FullConfig } from "@/lib/config";
import { saveConfig } from "@/stores/config";

/** Now, in seconds as History's times. */
const now = () => Math.floor(Date.now() / 1000);

/** The current draft as a favorite keeps it: its settings, pins, colors and
 * music. Every pane is named, so using the favorite clears what the current
 * draft has nothing in. */
const kept = (c: FullConfig) => ({
  settings: c.settings,
  place: c.place,
  colors: c.colors,
  audio: c.audio,
});

/** Saves the favorites `change` works out from the saved ones and the
 * current draft. Throws why it couldn't, such as a name already taken. */
const edit = (change: (all: Favorite[], config: FullConfig) => Favorite[]) =>
  saveConfig((c) => ({ favorites: change(c.favorites, c) }));

/** The current draft as a new favorite, at the top. */
export const saveFavorite = ({ name = "", note }: Label) =>
  edit((all, c) => [{ name, note, time: now(), ...kept(c) }, ...all]);

/** The favorite called `name` with the current draft, moved to the top. */
export const updateFavorite = (name: string) =>
  edit((all, c) => [
    ...all
      .filter((f) => f.name === name)
      .map((f) => ({ ...f, time: now(), ...kept(c) })),
    ...all.filter((f) => f.name !== name),
  ]);

/** Renames the favorite called `name` and sets its note, in place. */
export const relabelFavorite = (name: string, label: Label) =>
  edit((all) => all.map((f) => (f.name === name ? { ...f, ...label } : f)));

/** Deletes the favorite called `name`. */
export const deleteFavorite = (name: string) =>
  edit((all) => all.filter((f) => f.name !== name));
