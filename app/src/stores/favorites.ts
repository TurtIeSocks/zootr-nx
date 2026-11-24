import type { Favorite, Label } from "@/bindings";
import type { Settings } from "@/lib/config";
import { saveConfig } from "@/stores/config";

/** Now, in seconds as History's times. */
const now = () => Math.floor(Date.now() / 1000);

/** Saves the favorites `change` works out from the saved ones and the
 * current settings. Throws why it couldn't, such as a name already taken. */
const edit = (change: (all: Favorite[], settings: Settings) => Favorite[]) =>
  saveConfig((c) => ({ favorites: change(c.favorites, c.settings) }));

/** The current settings as a new favorite, at the top. */
export const saveFavorite = ({ name = "", note }: Label) =>
  edit((all, settings) => [{ name, note, time: now(), settings }, ...all]);

/** The favorite called `name` with the current settings, moved to the top. */
export const updateFavorite = (name: string) =>
  edit((all, settings) => [
    ...all
      .filter((f) => f.name === name)
      .map((f) => ({ ...f, time: now(), settings })),
    ...all.filter((f) => f.name !== name),
  ]);

/** Renames the favorite called `name` and sets its note, in place. */
export const relabelFavorite = (name: string, label: Label) =>
  edit((all) => all.map((f) => (f.name === name ? { ...f, ...label } : f)));

/** Deletes the favorite called `name`. */
export const deleteFavorite = (name: string) =>
  edit((all) => all.filter((f) => f.name !== name));
