import { Link2, Lock, RotateCcw } from "lucide-react";
import { Fragment, useMemo } from "react";
import type { SettingDef } from "@/bindings";
import { RequestError } from "@/components/request-error";
import { TipButton } from "@/components/tip-button";
import { Segmented } from "@/components/ui/segmented";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { affects, chosen, forcedPick, isOn, type Patch } from "@/lib/settings";
import { ApBadge, Row } from "@/settings/row";
import {
  controlId,
  heldPicks,
  labelId,
  segmented,
  useLockedBy,
  useUnmet,
} from "@/settings/setting";
import { revealSetting } from "@/shell/find/reveal";
import { changeSettings, useConfig } from "@/stores/config";
import {
  DEFAULTS_FROM,
  useDrifted,
  useDriftFrom,
  useDriftSettings,
} from "@/stores/seeds";

/** Props for {@link SettingRow}. */
export interface SettingRowProps {
  def: SettingDef;
}

/** One schema entry and its control, with a reset while it's changed and
 * the reason while a parent locks it. Its changes save one at a time, and
 * one that fails says why under it. */
export function SettingRow({ def }: SettingRowProps) {
  return (
    <RequestProvider>
      <SettingRowFrame def={def} />
    </RequestProvider>
  );
}

/** Props for {@link LockedBy}. */
export interface LockedByProps {
  /** The setting that forces this one on. */
  parent: SettingDef;
  /** The option it forces, for a choice. */
  pick?: string;
}

/** Props for {@link SettingLink}. */
interface SettingLinkProps {
  def: SettingDef;
}

/** A setting's name in a caption, which jumps to its row. */
function SettingLink({ def }: SettingLinkProps) {
  return (
    <button
      type="button"
      onClick={() => revealSetting(def)}
      className="cursor-pointer font-medium text-ink-muted underline-offset-2 hover:text-ink hover:underline focus-visible:underline focus-visible:outline-none"
    >
      {def.label}
    </button>
  );
}

/** "On because of Parent", or "Pick because of Parent" for a choice, under
 * a locked setting. */
export function LockedBy({ parent, pick }: LockedByProps) {
  return (
    <p className="mt-1.5 flex items-center gap-1 text-ink-faint text-xs">
      <Link2 className="size-3" />
      {pick ?? "On"} because of <SettingLink def={parent} />
    </p>
  );
}

/** Props for {@link Affects}. */
interface AffectsProps {
  /** The settings its picks can force. */
  forced: SettingDef[];
}

/** "Affects Other, Another", under a choice whose picks force others,
 * whatever it's picked now. */
function Affects({ forced }: AffectsProps) {
  return (
    <p className="mt-1.5 flex items-center gap-1 text-ink-faint text-xs">
      <Link2 className="size-3" />
      Affects
      <span>
        {forced.map((d, i) => (
          <Fragment key={d.id}>
            {i > 0 && ", "}
            <SettingLink def={d} />
          </Fragment>
        ))}
      </span>
    </p>
  );
}

/** Props for {@link Needs}. */
interface NeedsProps {
  /** The toggle that must be on first. */
  needed: SettingDef;
}

/** "Needs Other on first", under a setting that does nothing yet. */
function Needs({ needed }: NeedsProps) {
  return (
    <p className="mt-1.5 flex items-center gap-1 text-ink-faint text-xs">
      <Lock className="size-3" />
      Needs <SettingLink def={needed} /> on first
    </p>
  );
}

/** "Not in AP: Key rings, Keysanity", under a choice for each reason the
 * Archipelago world leaves some of its picks out. */
function HeldPicks({ def }: SettingRowProps) {
  return heldPicks(def).map(([mark, labels]) => (
    <p
      key={mark.why}
      className="mt-1.5 flex items-center gap-1.5 text-ink-faint text-xs"
    >
      <ApBadge mark={mark} />
      {labels.join(", ")}
    </p>
  ));
}

/** The row around a setting's control, inside its request. */
function SettingRowFrame({ def }: SettingRowProps) {
  const parent = useLockedBy(def);
  const pick = useConfig(
    (s) => forcedPick(def, s.config.settings, s.schema)?.label,
  );
  const needed = useUnmet(def);
  const schema = useConfig((s) => s.schema);
  const forced = useMemo(() => affects(def, schema), [def, schema]);
  const changed = useDrifted(def.id);
  return (
    <Row
      id={controlId(def)}
      label={def.label}
      description={def.description}
      changed={changed}
      locked={parent !== undefined || needed !== undefined}
      archipelago={def.archipelago}
      under={
        <>
          <HeldPicks def={def} />
          {parent && <LockedBy parent={parent} pick={pick} />}
          {needed && <Needs needed={needed} />}
          {forced.length > 0 && <Affects forced={forced} />}
          <RequestError className="mt-1.5" />
        </>
      }
    >
      {changed && <ResetSetting def={def} />}
      <SettingControl def={def} />
    </Row>
  );
}

/** Puts a setting back to what drift measures from: the last seed's
 * value, or its default. Shows on hover or focus. */
function ResetSetting({ def }: SettingRowProps) {
  const { run } = useRequest();
  const base = useDriftSettings();
  const from = useDriftFrom();
  const value = base[def.id] as Patch[string];
  return (
    <span className="opacity-0 transition-opacity duration-(--duration-fast) focus-within:opacity-100 group-hover/row:opacity-100">
      <TipButton
        label={`Reset ${def.label}`}
        tip={
          from === DEFAULTS_FROM
            ? "Back to its default"
            : `Back to ${from}'s value`
        }
        variant="ghost"
        onClick={() => run(() => changeSettings({ [def.id]: value }))}
      >
        <RotateCcw />
      </TipButton>
    </span>
  );
}

/** The one place a setting's kind becomes its control: a switch, options in
 * place, or a select. */
function SettingControl({ def }: SettingRowProps) {
  const { run } = useRequest();
  const on = useConfig((s) => isOn(def, s.config.settings, s.schema));
  const value = useConfig((s) => chosen(def, s.config.settings)?.id);
  const locked = useLockedBy(def) !== undefined;
  const forced = useConfig(
    (s) => forcedPick(def, s.config.settings, s.schema)?.id,
  );
  const off = useUnmet(def) !== undefined;
  // Unmet, a setting does nothing, so it shows off (no pick for a choice)
  // while its stored value waits for the setting it needs. Forced, it
  // shows the forced pick.
  const shown = forced ?? (off ? undefined : value);
  const set = (value: boolean | string) =>
    run(() => changeSettings({ [def.id]: value }));
  const { kind } = def;
  if (kind.type === "toggle") {
    return (
      <Switch
        id={controlId(def)}
        checked={on && !off}
        disabled={locked || off}
        onCheckedChange={set}
      />
    );
  }
  // The starting inventory's grid draws on its own, on the group page.
  if (kind.type === "inventory") return null;
  if (segmented(kind.options)) {
    return (
      <Segmented
        id={controlId(def)}
        aria-labelledby={labelId(def)}
        options={kind.options}
        value={shown}
        disabled={off || locked}
        onValueChange={set}
      />
    );
  }
  return (
    <Select value={shown ?? ""} disabled={off || locked} onValueChange={set}>
      <SelectTrigger id={controlId(def)}>
        <SelectValue placeholder="Off" />
      </SelectTrigger>
      <SelectContent>
        {kind.options.map((o) => (
          <SelectItem
            key={o.id}
            value={o.id}
            hint={o.id === kind.default ? "default" : undefined}
          >
            {o.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
