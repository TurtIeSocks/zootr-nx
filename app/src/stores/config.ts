import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { type BuiltIn, commands, type SettingDef } from "@/bindings";
import { call, keep, message } from "@/lib/api";
import { type FullConfig, full, testOn } from "@/lib/config";
import { type Patch, patched } from "@/lib/settings";
import { ownPane, shows } from "@/shell/nav";
import { once } from "@/stores/once";
import { useUi } from "@/stores/ui";

/** A schema group. Its change dot reads the drift list
 * (`useGroupDrifted`). */
export interface Group {
  name: string;
}

/** The randomizer's settings schema and the saved config. `saved_tests` and
 * `settings` are keyed by names the maintainer types, which may hold a
 * dot, so they are read and written whole, never by a path through a key. */
export interface ConfigState {
  /** Every setting the randomizer has, in order. */
  schema: SettingDef[];
  /** The favorites that come with the app. */
  builtIns: BuiltIn[];
  /** The saved config, with every field filled in. */
  config: FullConfig;
  /** The config has been read once. */
  loaded: boolean;
  /** Why reading the schema or the config last failed. */
  error?: string;
  /** The schema's groups, derived from `schema` and `config`. */
  groups: Group[];
  /** The test setup changes anything. */
  testOn: boolean;
}

/** The config store. Start it with {@link init}. */
export const useConfig = create<ConfigState>()(
  dotPath(
    (): ConfigState => ({
      schema: [],
      builtIns: [],
      config: full({}),
      loaded: false,
      groups: [],
      testOn: false,
    }),
  ),
);

/** Works out `groups` and `testOn` from the schema and config. A saved
 * section whose group the schema no longer has, like a panel folded into
 * another, opens the first group instead of an empty page. */
function derive() {
  const { schema, config } = useConfig.getState();
  const groups = [...new Set(schema.map((d) => d.group))].map((name) => ({
    name,
  }));
  useConfig.setPath("groups", keep(groups));
  // A hidden test setup doesn't reach seeds, so it isn't on either.
  useConfig.setPath("testOn", config.dev_tools && testOn(config.test));
  const { section } = useUi.getState();
  const gone =
    (groups.length > 0 &&
      !ownPane(section) &&
      !groups.some((g) => g.name === section)) ||
    !shows(section, config.dev_tools);
  if (gone) useUi.setPath("section", groups[0]?.name ?? "settings");
}
useConfig.subscribePath("schema", derive);
useConfig.subscribePath("config", derive);

/** Whether developer tools show. */
export const useDevTools = () => useConfig((s) => s.config.dev_tools);

/** Reads the config again. An unchanged one keeps its identity, so open
 * forms keep their edits. */
export async function reload() {
  try {
    useConfig.setPath("config", keep(full(await call(commands.config()))));
    useConfig.setPath("loaded", true);
  } catch (e) {
    useConfig.setPath("error", message(e));
  }
}

/** Reads the schema and config, and the config again whenever the window
 * gets focus. */
const started = once(async () => {
  try {
    useConfig.setPath("schema", await commands.schema());
    useConfig.setPath("builtIns", await commands.builtInFavorites());
  } catch (e) {
    useConfig.setPath("error", message(e));
  }
  await reload();
  // The config file can change outside the app.
  const refresh = () => void reload();
  window.addEventListener("focus", refresh);
  return () => window.removeEventListener("focus", refresh);
});

export const { init, stop } = started;

/** The last save still to finish, if one is. */
let saving: Promise<void> | undefined;

/** Saves the config with `patch` laid over it. Saves run one after another,
 * and a patch given as a function is worked out from the config the save
 * before it left, so a quick second change never undoes the first. Returns
 * what the save changed on its own (a target moved out of the game's mods
 * root), and throws what saving failed with. */
export function saveConfig(
  patch: Partial<FullConfig> | ((config: FullConfig) => Partial<FullConfig>),
): Promise<string[]> {
  const run = async () => {
    const before = useConfig.getState().config;
    const config = {
      ...before,
      ...(typeof patch === "function" ? patch(before) : patch),
    };
    const notes = await call(commands.setConfig(config));
    useConfig.setPath("config", keep(config));
    // The save changed what was sent, so read back what it wrote.
    if (notes.length > 0) await reload();
    return notes;
  };
  // Starts at once when nothing is saving, else after the last save.
  const save = saving ? saving.then(run) : run();
  const done: Promise<void> = save
    .catch(() => {})
    .then(() => {
      if (saving === done) saving = undefined;
    });
  saving = done;
  return save;
}

/** Saves the settings with `patch` laid over them. */
export const changeSettings = (patch: Patch) =>
  saveConfig((c) => ({ settings: patched(c.settings, patch) }));
