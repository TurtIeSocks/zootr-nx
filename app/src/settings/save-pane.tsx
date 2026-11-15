import { Loader2 } from "lucide-react";
import { useState } from "react";
import {
  commands,
  type SaveEdit,
  type SaveOptions,
  type SavePreset,
} from "@/bindings";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { Switch } from "@/components/ui/switch";
import { call } from "@/lib/api";
import { useRequestState } from "@/lib/request";
import { useLoad } from "@/lib/use-load";
import { Row } from "@/settings/row";
import { SettingsSection } from "@/settings/settings-section";
import { PageHeader } from "@/shell/page-header";
import { togglePreset, useUi } from "@/stores/ui";

/** Presets applied to an existing save, for testing without a new game. */
export function SavePane() {
  const options = useLoad(loadOptions, null);
  return (
    <>
      <PageHeader
        title="Save slot"
        description="Sets a preset's flags on a save you already have. Close the game first. The old save is kept next to it as a backup. Presets that hand over items only work on a new game, so they aren't here."
      />
      {options.status === "ok" ? (
        <SaveForm options={options.data} />
      ) : options.status === "error" ? (
        <InlineError error={options.error} />
      ) : (
        <Loader2 className="size-4 animate-spin text-ink-muted" />
      )}
    </>
  );
}

/** The save slots and presets there are. */
const loadOptions = () => call(commands.saveOptions());

/** Props for {@link SaveForm}. */
interface SaveFormProps {
  options: SaveOptions;
}

/** The slot, the presets and Apply, once the options are read. */
function SaveForm({ options }: SaveFormProps) {
  return (
    <>
      <Card>
        <div className="p-4">
          <SlotPicker slots={options.slots} />
        </div>
      </Card>
      <SettingsSection>
        {options.presets.map((p) => (
          <PresetRow key={p.name} preset={p} />
        ))}
      </SettingsSection>
      <Apply />
    </>
  );
}

/** Props for {@link SlotPicker}. */
interface SlotPickerProps {
  /** The slots that hold a save. */
  slots: number[];
}

/** Which save slot to change. */
function SlotPicker({ slots }: SlotPickerProps) {
  const [slot, setSlot] = useUi.usePath("save.slot");
  return (
    <Field labelId="save-slot" label="Slot">
      {slots.length > 0 ? (
        <Segmented
          aria-labelledby="save-slot"
          className="self-start"
          options={slots.map((n) => ({ id: n, label: `Slot ${n}` }))}
          value={slot ?? undefined}
          onValueChange={setSlot}
        />
      ) : (
        <p className="text-ink-muted text-xs">
          No saves in the save folder yet
        </p>
      )}
    </Field>
  );
}

/** Props for {@link PresetRow}. */
interface PresetRowProps {
  preset: SavePreset;
}

/** One preset and whether it's picked. */
function PresetRow({ preset }: PresetRowProps) {
  const id = `save-${preset.name}`;
  const picked = useUi((s) => s.save.presets.includes(preset.name));
  return (
    <Row id={id} label={preset.label} description={preset.what}>
      <Switch
        id={id}
        checked={picked}
        onCheckedChange={(on) => togglePreset(preset.name, on)}
      />
    </Row>
  );
}

/** Applies the picked presets to the picked slot, then shows what changed
 * and where the backup went. */
function Apply() {
  const [slot] = useUi.usePath("save.slot");
  const [picked] = useUi.usePath("save.presets");
  const apply = useRequestState();
  /** The last edit's log and backup. */
  const [done, setDone] = useState<SaveEdit>();
  return (
    <>
      <div>
        <Button
          disabled={apply.busy || slot === null || !picked.length}
          onClick={() =>
            apply.run(async () => {
              setDone(undefined);
              setDone(await call(commands.editSave(slot ?? 0, picked)));
            })
          }
        >
          {apply.busy && <Loader2 className="animate-spin" />}
          Apply
        </Button>
      </div>
      <InlineError error={apply.error} />
      {done && (
        <div className="flex flex-col gap-2 text-[13px]">
          <pre className="type-mono overflow-auto rounded-md bg-surface-sunken p-3">
            {done.log.join("\n")}
          </pre>
          <p className="text-ink-muted">Backup at {done.backup}</p>
        </div>
      )}
    </>
  );
}
