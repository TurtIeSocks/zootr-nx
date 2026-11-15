import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { commands, type SettingDef } from "@/bindings";
import { call, keep, message } from "@/lib/api";
import { type FullConfig, full, testOn } from "@/lib/config";
import { isChanged, type Patch, patched } from "@/lib/settings";
import { ownPane } from "@/shell/nav";
import { once } from "@/stores/once";
import { useUi } from "@/stores/ui";

/** A schema group, and whether it holds a change. */
export interface Group {
  name: string;
  /** A setting in it differs from its default. */
  changed: boolean;
}

/** The randomizer's settings schema and the saved config. `saved_tests` and
 * `settings` are keyed by names the maintainer types, which may hold a
 * dot, so they are read and written whole, never by a path through a key. */
export interface ConfigState {
  /** Every setting the randomizer has, in order. */
  schema: SettingDef[];
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
    changed: schema.some(
      (d) => d.group === name && isChanged(d, config.settings),
    ),
  }));
  useConfig.setPath("groups", keep(groups));
  useConfig.setPath("testOn", testOn(config.test));
  const { section } = useUi.getState();
  const gone =
    groups.length > 0 &&
    !ownPane(section) &&
    !groups.some((g) => g.name === section);
  if (gone) useUi.setPath("section", groups[0]?.name ?? "settings");
}
useConfig.subscribePath("schema", derive);
useConfig.subscribePath("config", derive);

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
 * before it left, so a quick second change never undoes the first. Throws
 * what saving failed with. */
export function saveConfig(
  patch: Partial<FullConfig> | ((config: FullConfig) => Partial<FullConfig>),
): Promise<void> {
  const run = async () => {
    const before = useConfig.getState().config;
    const config = {
      ...before,
      ...(typeof patch === "function" ? patch(before) : patch),
    };
    await call(commands.setConfig(config));
    useConfig.setPath("config", keep(config));
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
