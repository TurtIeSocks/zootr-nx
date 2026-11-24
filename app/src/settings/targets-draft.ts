import type { Release, Target } from "@/bindings";
import type { TopicId } from "@/help/topics";
import { splitLines } from "@/lib/format";

/** A target as the form edits it: every field a string. */
export interface Draft {
  name: string;
  app: string;
  /** One launch argument per row. */
  args: string;
  mod_dir: string;
  save_dir: string;
  line_file: string;
  /** One settings line per row. */
  lines: string;
  control: string;
  bridge: string;
  /** One env line per row. */
  env: string;
  /** One harness env line per row. */
  harness_env: string;
  /** One rebuild command argument per row. */
  rebuild_command: string;
  manifest: string;
  /** The build the target runs, "" for either. Shown, not edited. */
  release: Release | "";
}

/** A saved target as the form edits it. */
export const toDraft = (t: Target): Draft => ({
  name: t.name,
  app: t.app,
  args: t.args?.join("\n") ?? "",
  mod_dir: t.mod_dir,
  save_dir: t.save_dir ?? "",
  line_file: t.settings_file?.file ?? "",
  lines: t.settings_file?.lines?.join("\n") ?? "",
  control: t.control ?? "",
  bridge: t.bridge ?? "",
  env: t.env?.join("\n") ?? "",
  harness_env: t.harness_env?.join("\n") ?? "",
  rebuild_command: t.rebuild?.command?.join("\n") ?? "",
  manifest: t.rebuild?.manifest ?? "",
  release: t.release ?? "",
});

/** The target a draft saves as, trimmed, with blank options left out. */
export const toTarget = (d: Draft): Target => ({
  name: d.name.trim(),
  app: d.app.trim(),
  args: splitLines(d.args),
  mod_dir: d.mod_dir.trim(),
  save_dir: d.save_dir.trim() || null,
  settings_file:
    d.line_file.trim() && splitLines(d.lines).length
      ? { file: d.line_file.trim(), lines: splitLines(d.lines) }
      : null,
  control: d.control.trim() || null,
  bridge: d.bridge.trim() || null,
  env: splitLines(d.env),
  harness_env: splitLines(d.harness_env),
  rebuild:
    splitLines(d.rebuild_command).length || d.manifest.trim()
      ? {
          command: splitLines(d.rebuild_command),
          manifest: d.manifest.trim() || null,
        }
      : null,
  release: d.release || null,
});

/** A new target with nothing filled in. */
export const BLANK: Draft = toDraft({ name: "", app: "", mod_dir: "" });

/** `t` as a draft to add beside `drafts`, numbered past any name already
 * taken. */
export function importedDraft(drafts: Draft[], t: Target): Draft {
  const taken = new Set(drafts.map((d) => d.name.trim()));
  let name = t.name;
  for (let n = 2; taken.has(name); n++) name = `${t.name} ${n}`;
  return toDraft({ ...t, name });
}

/** Whether `d` is what's saved under its name, so its export is what shows. */
export const isSaved = (targets: Target[], d: Draft) => {
  const t = targets.find((t) => t.name === d.name.trim());
  return t !== undefined && JSON.stringify(toDraft(t)) === JSON.stringify(d);
};

/** Whether the drafts differ from the saved targets, so there's a save to make. */
export const isEdited = (targets: Target[], drafts: Draft[]) =>
  JSON.stringify(targets.map(toDraft)) !== JSON.stringify(drafts);

/** One edited field of a target. */
export interface DraftField {
  key: keyof Draft;
  label: string;
  /** What goes there, said under the field. */
  hint: string;
  /** A help bubble beside the label. */
  help?: TopicId;
  /** What its Browse button picks, if it has one. */
  pick?: "folder" | "file";
  /** One entry per line. */
  multiline?: boolean;
  /** Shown only with developer tools on. */
  dev?: boolean;
}

/** The entries of `list` that show, with developer tools on or off. */
export const shown = <T extends { dev?: boolean }>(
  list: T[],
  devTools: boolean,
) => list.filter((x) => devTools || !x.dev);

/** The fields each target card always shows, in order. */
export const FIELDS: DraftField[] = [
  { key: "name", label: "Name", hint: "Shown in Play's picker" },
  { key: "app", label: "App", hint: "What Play opens", pick: "file" },
  {
    key: "mod_dir",
    help: "mod-folder",
    label: "Mod Folder",
    hint: "The folder the game loads this mod from",
    pick: "folder",
  },
  {
    key: "save_dir",
    label: "Save Folder (Optional)",
    hint: "Where the game keeps its saves. The Save Editor and Items read them here",
    pick: "folder",
  },
];

/** The fields under a card's Advanced section, in order. */
export const ADVANCED_FIELDS: DraftField[] = [
  {
    key: "args",
    help: "variables",
    label: "Launch Arguments (Optional)",
    hint: "Extra arguments, for apps that take the game on the command line, one argument per line",
    multiline: true,
  },
  {
    key: "line_file",
    label: "Settings File (Optional)",
    hint: "For apps that read the mod path from their own settings file: Install writes the lines below into it",
    pick: "file",
  },
  {
    key: "lines",
    help: "variables",
    label: "Settings Lines (Optional)",
    hint: "One KEY=VALUE per line",
    multiline: true,
  },
  {
    key: "bridge",
    help: "control-socket",
    label: "Control Socket (Optional)",
    hint: "A socket the target opens for zootr",
  },
  {
    key: "env",
    help: "variables",
    label: "Environment (Optional)",
    hint: "One NAME=VALUE per line, set when Play starts the app",
    multiline: true,
  },
  {
    key: "rebuild_command",
    dev: true,
    help: "variables",
    label: "Rebuild Command (Optional)",
    hint: "For apps that build the mod into the game: what to run after Install, one argument per line",
    multiline: true,
  },
  {
    key: "manifest",
    dev: true,
    label: "Patch List (Optional)",
    hint: "The build's exefs_patches.txt, when it isn't in the game folder that holds the mod folder",
    pick: "file",
  },
  {
    key: "harness_env",
    dev: true,
    help: "variables",
    label: "Harness Environment (Optional)",
    hint: "One NAME=VALUE per line, for scripted play only, never Play",
    multiline: true,
  },
  {
    key: "control",
    dev: true,
    label: "Control File (Optional)",
    hint: "A file the game reads input lines from, for scripted play",
    pick: "file",
  },
];

/** One variable a target field can name, and what it becomes. */
export interface Variable {
  name: string;
  /** The fields it works in. */
  in: string;
  /** More fields it works in, named only with developer tools on. */
  devIn?: string;
  becomes: string;
  /** Works only in fields developer tools show. */
  dev?: boolean;
}

/** Every variable zootr fills in a target's fields. */
export const VARIABLES: Variable[] = [
  {
    name: "{game}",
    in: "Launch Arguments",
    devIn: "Rebuild Command",
    becomes: "Your game dump",
  },
  {
    name: "{romfs}",
    in: "Settings Lines",
    becomes: "The mod's game files, in the mod folder",
  },
  {
    name: "{cheats}",
    in: "Settings Lines",
    becomes:
      "The mod's cheat folder, in the mod folder. Naming it tells zootr the app loads the cheat file",
  },
  { name: "{bridge}", in: "Environment", becomes: "The control socket" },
  {
    name: "{control}",
    in: "Settings Lines",
    becomes: "The control file",
    dev: true,
  },
  {
    name: "{save}",
    in: "Harness Environment",
    becomes: "The run's own save folder",
    dev: true,
  },
];

/** The fields a variable works in, as listed with developer tools on or off. */
export const worksIn = (v: Variable, devTools: boolean) =>
  [v.in, devTools && v.devIn].filter(Boolean).join(", ");

/** Whether a draft names any variable, so Variables opens for it. */
export const namesVariable = (draft: Draft) =>
  [...FIELDS, ...ADVANCED_FIELDS].some((f) =>
    VARIABLES.some((v) => String(draft[f.key]).includes(v.name)),
  );

/** Why the drafts can't be saved as they are, or "" when they can. */
export function problem(drafts: Draft[]): string {
  const names = drafts.map((d) => d.name.trim());
  if (
    drafts.some((d) => !d.name.trim() || !d.app.trim() || !d.mod_dir.trim())
  ) {
    return "Each target needs a name, an app and a mod folder";
  }
  if (new Set(names).size !== names.length) return "Target names must differ";
  if (drafts.some((d) => splitLines(d.lines).some((l) => !l.includes("=")))) {
    return "A settings line looks like KEY={romfs}";
  }
  const env = (d: Draft) => [
    ...splitLines(d.env),
    ...splitLines(d.harness_env),
  ];
  if (drafts.some((d) => env(d).some((l) => !l.includes("=")))) {
    return "An environment line looks like NAME=VALUE";
  }
  if (
    drafts.some(
      (d) => !d.bridge.trim() && env(d).some((l) => l.includes("{bridge}")),
    )
  ) {
    return "An environment line names {bridge}, so the target needs a control socket";
  }
  return "";
}
