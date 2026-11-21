import { Link2, Lock, RotateCcw } from "lucide-react";
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
import { chosen, isOn, type Patch } from "@/lib/settings";
import { Row } from "@/settings/row";
import {
  controlId,
  labelId,
  segmented,
  useLockedBy,
  useUnmet,
} from "@/settings/setting";
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
}

/** "On because of Parent", under a locked setting. */
export function LockedBy({ parent }: LockedByProps) {
  return (
    <p className="mt-1.5 flex items-center gap-1 text-ink-faint text-xs">
      <Link2 className="size-3" />
      On because of <b className="font-medium text-ink-muted">{parent.label}</b>
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
      Needs <b className="font-medium text-ink-muted">{needed.label}</b> on
      first
    </p>
  );
}

/** The row around a setting's control, inside its request. */
function SettingRowFrame({ def }: SettingRowProps) {
  const parent = useLockedBy(def);
  const needed = useUnmet(def);
  const changed = useDrifted(def.id);
  return (
    <Row
      id={controlId(def)}
      label={def.label}
      description={def.description}
      changed={changed}
      locked={parent !== undefined || needed !== undefined}
      under={
        <>
          {parent && <LockedBy parent={parent} />}
          {needed && <Needs needed={needed} />}
          <RequestError className="mt-1.5" />
        </>
      }
    >
      {changed && <ResetSetting def={def} />}
      <SettingControl def={def} />
    </Row>
  );
}

/** Puts a setting back to what drift measures from: the installed seed's
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
          from === DEFAULTS_FROM ? "Back to its default" : `Back to ${from}'s`
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
  const off = useUnmet(def) !== undefined;
  const set = (value: boolean | string) =>
    run(() => changeSettings({ [def.id]: value }));
  const { kind } = def;
  if (kind.type === "toggle") {
    return (
      <Switch
        id={controlId(def)}
        checked={on}
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
        aria-labelledby={labelId(def)}
        options={kind.options}
        value={value}
        disabled={off}
        onValueChange={set}
      />
    );
  }
  return (
    <Select value={value} disabled={off} onValueChange={set}>
      <SelectTrigger id={controlId(def)}>
        <SelectValue />
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
