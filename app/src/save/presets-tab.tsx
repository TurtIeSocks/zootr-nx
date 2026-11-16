import type { SavePreset } from "@/bindings";
import { Switch } from "@/components/ui/switch";
import { togglePreset } from "@/save/tabs";
import { Row } from "@/settings/row";
import { SettingsSection } from "@/settings/settings-section";
import { useUi } from "@/stores/ui";

/** Props for {@link PresetsTab}. */
interface PresetsTabProps {
  presets: SavePreset[];
}

/** The presets a save can take. Turning one on stages its edits, which
 * show in Flags; turning it off drops them. Presets that hand over items
 * or move a quest on only work on a new game, so they aren't here. */
export function PresetsTab({ presets }: PresetsTabProps) {
  return (
    <SettingsSection>
      {presets.map((p) => (
        <PresetRow key={p.name} preset={p} />
      ))}
    </SettingsSection>
  );
}

/** One preset and whether it's staged. */
function PresetRow({ preset }: { preset: SavePreset }) {
  const id = `save-${preset.name}`;
  const on = useUi((s) => s.save.presets.includes(preset.name));
  return (
    <Row id={id} label={preset.label} description={preset.what}>
      <Switch
        id={id}
        checked={on}
        onCheckedChange={(c) => togglePreset(preset.name, c)}
      />
    </Row>
  );
}
