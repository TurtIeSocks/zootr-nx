import type { Target } from "@/bindings";
import type { TopicId } from "@/help/topics";
import { splitLines } from "@/lib/format";

/** A target as the form edits it: every field a string. */
export interface Draft {
  name: string;
  app: string;
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
}

/** A saved target as the form edits it. */
export const toDraft = (t: Target): Draft => ({
  name: t.name,
  app: t.app,
  args: t.args?.join(" ") ?? "",
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
});

/** The target a draft saves as, trimmed, with blank options left out. */
export const toTarget = (d: Draft): Target => ({
  name: d.name.trim(),
  app: d.app.trim(),
  args: d.args.split(/\s+/).filter(Boolean),
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
});

/** A new target with nothing filled in. */
export const BLANK: Draft = toDraft({ name: "", app: "", mod_dir: "" });

/** The drafts with `t` added, numbered past any name already taken. */
export function withTarget(drafts: Draft[], t: Target): Draft[] {
  const taken = new Set(drafts.map((d) => d.name.trim()));
  let name = t.name;
  for (let n = 2; taken.has(name); n++) name = `${t.name} ${n}`;
  return [...drafts, toDraft({ ...t, name })];
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
  /** The placeholder: what goes there. */
  hint: string;
  /** A help bubble beside the label. */
  help?: TopicId;
  /** What its Browse button picks, if it has one. */
  pick?: "folder" | "file";
  /** One entry per line. */
  multiline?: boolean;
}

/** The fields each target card shows, in order. */
export const FIELDS: DraftField[] = [
  { key: "name", label: "Name", hint: "Shown in the Play picker" },
  {
    key: "app",
    label: "App",
    hint: "What Play opens",
    pick: "file",
  },
  {
    key: "args",
    label: "Launch arguments (optional)",
    hint: "-g {game}: {game} becomes your game dump",
  },
  {
    key: "mod_dir",
    help: "mod-folder",
    label: "Mod folder",
    hint: "The folder the game loads this mod from. Install writes romfs/ here",
    pick: "folder",
  },
  {
    key: "save_dir",
    label: "Save folder (optional)",
    hint: "The folder holding slot_00, slot_01 and so on",
    pick: "folder",
  },
  {
    key: "line_file",
    label: "Settings file (optional)",
    hint: "For targets that read the mod path from a settings file",
    pick: "file",
  },
  {
    key: "lines",
    label: "Settings lines (optional)",
    hint: "One KEY=VALUE per line. {romfs} becomes the mod's romfs folder, {cheats} its cheat folder, {control} the control file",
    multiline: true,
  },
  {
    key: "bridge",
    help: "control-socket",
    label: "Control socket (optional)",
    hint: "For targets with a control socket: live reads and dev tools go through it",
  },
  {
    key: "env",
    label: "Environment (optional)",
    hint: "One NAME=VALUE per line, set when Play starts the app. {bridge} becomes the control socket",
    multiline: true,
  },
  {
    key: "harness_env",
    label: "Harness environment (optional)",
    hint: "One NAME=VALUE per line, added when an agent's play harness starts the app, never Play. {save} becomes the run's save folder",
    multiline: true,
  },
  {
    key: "control",
    label: "Control file (optional)",
    hint: "A file the game reads input lines from, for scripted play",
    pick: "file",
  },
];

/** The fields of a game that bakes the mods' exefs patches in when it's
 * built, in their own section. */
export const BUILD_FIELDS: DraftField[] = [
  {
    key: "rebuild_command",
    label: "Rebuild command (optional)",
    hint: "One argument per line: what rebuilds the game. {game} becomes your game dump",
    multiline: true,
  },
  {
    key: "manifest",
    label: "Patch list (optional)",
    hint: "The build's exefs_patches.txt, when it isn't in the game folder that holds the mod folder",
    pick: "file",
  },
];

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
