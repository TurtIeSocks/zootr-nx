import { Loader2 } from "lucide-react";
import { useState } from "react";
import { InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { Field } from "@/components/field";
import { PathInput } from "@/components/path-input";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { FullConfig } from "@/lib/config";
import { useRequestState } from "@/lib/request";
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
      <GameDump key={game} />
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
  const save = useRequestState();
  return (
    <SettingsSection>
      <Row
        id="game-assets"
        label="Icons from the game dump"
        description="Draws the item tracker's icons from your own copy of the game instead of zootr-nx's. zootr-nx doesn't include any of the game's art: the icons are read from your game dump, on this computer"
        under={<InlineError error={save.error} />}
      >
        <Tooltip open={dump ? false : undefined}>
          <TooltipTrigger asChild>
            {/* A disabled switch takes no hover, so its wrapper says why. */}
            <span tabIndex={dump ? undefined : 0}>
              <Switch
                id="game-assets"
                checked={checked}
                disabled={!dump || save.busy}
                onCheckedChange={(on) =>
                  save.run(() => saveConfig({ game_assets: on }))
                }
              />
            </span>
          </TooltipTrigger>
          <TooltipContent>Set the game dump first</TooltipContent>
        </Tooltip>
      </Row>
    </SettingsSection>
  );
}

/** Props for {@link SwitchRow}. */
interface SwitchRowProps {
  id: string;
  label: string;
  description: string;
  checked: boolean;
  /** The config change turning it `on` or off saves. */
  change: (on: boolean) => Partial<FullConfig>;
}

/** A row whose switch saves one app setting, saying why if it can't. */
function SwitchRow({
  id,
  label,
  description,
  checked,
  change,
}: SwitchRowProps) {
  const save = useRequestState();
  return (
    <SettingsSection>
      <Row
        id={id}
        label={label}
        description={description}
        under={<InlineError error={save.error} />}
      >
        <Switch
          id={id}
          checked={checked}
          disabled={save.busy}
          onCheckedChange={(on) => save.run(() => saveConfig(change(on)))}
        />
      </Row>
    </SettingsSection>
  );
}

/** Shows or hides everything only developers need. */
function DevTools() {
  const [checked] = useConfig.usePath("config.dev_tools");
  return (
    <SwitchRow
      id="dev-tools"
      label="Developer tools"
      description="Shows the dev panel, Test setup and the targets' fields for scripted play. Off, a hidden setup leaves your seeds alone."
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
      id="update-check"
      label="Check for updates"
      description="Asks GitHub for the latest release at launch and every 6 hours, and the sidebar says when a newer one is out. Off, the app never contacts GitHub."
      checked={!off}
      change={(on) => ({ no_update_check: !on })}
    />
  );
}

/** The game dump's path. Starts from the saved one; Save extracts it. */
function GameDump() {
  const [saved] = useConfig.usePath("config.game");
  const [game, setGame] = useState(saved ?? "");
  const save = useRequestState();

  return (
    <Card>
      <div className="flex flex-col gap-1.5 p-4">
        <Field
          id="game"
          label="Game dump"
          help="game-dump"
          hint="The .nup of your own copy of the game, with the 1.1.1 update. Save extracts the files zootr reads into a cache, which takes a few seconds the first time. A folder of already extracted romfs works too: paste its path."
        >
          <div className="flex gap-2">
            <PathInput
              id="game"
              pick="file"
              extensions={["nup"]}
              placeholder="/path/to/oot.nup"
              value={game}
              onChange={setGame}
            />
            <Button
              onClick={() =>
                save.run(() => saveConfig({ game: game.trim() || null }))
              }
              disabled={save.busy || game === (saved ?? "")}
            >
              {save.busy && <Loader2 className="animate-spin" />}
              {save.busy ? "Extracting" : "Save"}
            </Button>
          </div>
        </Field>
        <InlineError error={save.error} />
      </div>
    </Card>
  );
}
