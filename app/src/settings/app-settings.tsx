import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { FullConfig } from "@/lib/config";
import { useRequestState } from "@/lib/request";
import { APP_FIELDS, type AppField } from "@/settings/app-fields";
import { GameDump } from "@/settings/game-dump";
import { Row } from "@/settings/row";
import { SettingsSection } from "@/settings/settings-section";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig } from "@/stores/config";

/** The app's own settings, apart from the randomizer's. The dump field
 * starts again when a dump is saved. */
export function AppSettings() {
  const [game] = useConfig.usePath("config.game");
  return (
    <>
      <PageHeader title="Settings" />
      <Card>
        <div className="p-4">
          <GameDump key={game} />
        </div>
      </Card>
      <GameAssets />
      <UpdateCheck />
      <DevTools />
    </>
  );
}

/** Whether the item tracker draws its icons from the dump. Off until the
 * person turns it on, since zootr-nx ships none of the game's art. Needs a
 * dump, and says so while there's none. */
function GameAssets() {
  const [checked] = useConfig.usePath("config.game_assets");
  const dump = useConfig((s) => s.config.game !== null);
  return (
    <SwitchRow
      field={APP_FIELDS.gameAssets}
      checked={checked}
      change={(on) => ({ game_assets: on })}
      blocked={dump ? undefined : "Set the game dump first"}
    />
  );
}

/** Props for {@link SwitchRow}. */
interface SwitchRowProps {
  field: AppField;
  checked: boolean;
  /** The config change turning it `on` or off saves. */
  change: (on: boolean) => Partial<FullConfig>;
  /** Why it can't change now, if it can't: the switch is off limits and
   * says so on hover. */
  blocked?: string;
}

/** A row whose switch saves one app setting, saying why if it can't. */
function SwitchRow({ field, checked, change, blocked }: SwitchRowProps) {
  const save = useRequestState();
  return (
    <SettingsSection>
      <Row {...field} under={<InlineError error={save.error} />}>
        <Tooltip open={blocked ? undefined : false}>
          <TooltipTrigger asChild>
            {/* A disabled switch takes no hover, so its wrapper says why. */}
            <span tabIndex={blocked ? 0 : undefined}>
              <Switch
                id={field.id}
                checked={checked}
                disabled={!!blocked || save.busy}
                onCheckedChange={(on) => save.run(() => saveConfig(change(on)))}
              />
            </span>
          </TooltipTrigger>
          <TooltipContent>{blocked}</TooltipContent>
        </Tooltip>
      </Row>
    </SettingsSection>
  );
}

/** Shows or hides everything only developers need. */
function DevTools() {
  const [checked] = useConfig.usePath("config.dev_tools");
  return (
    <SwitchRow
      field={APP_FIELDS.devTools}
      checked={checked}
      change={(on) => ({ dev_tools: on })}
    />
  );
}

/** Whether the app asks GitHub for a newer release. */
function UpdateCheck() {
  const [off] = useConfig.usePath("config.no_update_check");
  return (
    <SwitchRow
      field={APP_FIELDS.updateCheck}
      checked={!off}
      change={(on) => ({ no_update_check: !on })}
    />
  );
}
