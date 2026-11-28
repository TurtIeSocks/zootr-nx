import type { SettingDef, Summary } from "@/bindings";

/** A History entry: an empty seed, changed by `patch`. */
export const summary = (patch: Partial<Summary> = {}): Summary => ({
  id: 1,
  time: 0,
  seed: "42",
  icons: [],
  share: "",
  settings: {},
  label: {},
  chests: 0,
  changed: 0,
  kept: null,
  test: false,
  plando: false,
  ap_slot: null,
  ap_server: null,
  death_link: false,
  release: "1.1.1",
  colors: {},
  audio: {},
  place: {},
  starred: false,
  ...patch,
});

/** A toggle setting `id`, off by default and labelled `id`, changed by
 * `patch`. */
export const toggleDef = (
  id: string,
  patch: Partial<SettingDef> = {},
): SettingDef => ({
  id,
  group: "g",
  label: id,
  description: "",
  kind: { type: "toggle", default: false },
  parent: null,
  requires: null,
  section: "Fake section",
  section_about: "Fake about",
  list: false,
  gives: [],
  archipelago: { ap: "offered" },
  ...patch,
});
