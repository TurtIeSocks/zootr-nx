import { Link2, RotateCcw } from "lucide-react";
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
import type { TopicId } from "@/help/topics";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { chosen, isChanged, isOn } from "@/lib/settings";
import { Row } from "@/settings/row";
import { controlId, labelId, segmented, useLockedBy } from "@/settings/setting";
import { changeSettings, useConfig } from "@/stores/config";

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

/** The settings whose rows carry a bubble, by id. */
const SETTING_HELP: Partial<Record<string, { help: TopicId }>> = {
  "start-inventory": { help: "starting-inventory" },
};

/** The row around a setting's control, inside its request. */
function SettingRowFrame({ def }: SettingRowProps) {
  const parent = useLockedBy(def);
  const own = useConfig((s) => isChanged(def, s.config.settings));
  const changed = !parent && own;
  return (
    <Row
      id={controlId(def)}
      label={def.label}
      help={SETTING_HELP[def.id]?.help}
      description={def.description}
      changed={changed}
      locked={parent !== undefined}
      under={
        <>
          {parent && <LockedBy parent={parent} />}
          <RequestError className="mt-1.5" />
        </>
      }
    >
      {changed && <ResetSetting def={def} />}
      <SettingControl def={def} />
    </Row>
  );
}

/** Puts a changed setting back to its default. Shows on hover or focus. */
function ResetSetting({ def }: SettingRowProps) {
  const { run } = useRequest();
  return (
    <span className="opacity-0 transition-opacity duration-(--duration-fast) focus-within:opacity-100 group-hover/row:opacity-100">
      <TipButton
        label={`Reset ${def.label}`}
        tip="Back to its default"
        variant="ghost"
        onClick={() => run(() => changeSettings({ [def.id]: undefined }))}
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
  const set = (value: boolean | string) =>
    run(() => changeSettings({ [def.id]: value }));
  const { kind } = def;
  if (kind.type === "toggle") {
    return (
      <Switch
        id={controlId(def)}
        checked={on}
        disabled={locked}
        onCheckedChange={set}
      />
    );
  }
  // The starting inventory's grid draws under its row, on the group page.
  if (kind.type === "inventory") return null;
  if (segmented(kind.options)) {
    return (
      <Segmented
        aria-labelledby={labelId(def)}
        options={kind.options}
        value={value}
        onValueChange={set}
      />
    );
  }
  return (
    <Select value={value} onValueChange={set}>
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
