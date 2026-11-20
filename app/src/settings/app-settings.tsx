import { Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
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
import { useRequestState } from "@/lib/request";
import { cn } from "@/lib/utils";
import { Row } from "@/settings/row";
import { SettingsSection } from "@/settings/settings-section";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig } from "@/stores/config";
import { useShell } from "@/stores/shell";

/** The app's own settings, apart from the randomizer's. Each part starts
 * again when what it shows changes: the field when a dump is saved, the dev
 * panel toggle's pulse at each new `shell.beckon`. */
export function AppSettings() {
  const [game] = useConfig.usePath("config.game");
  const [beckon] = useShell.usePath("beckon");
  return (
    <>
      <PageHeader title="Settings" />
      <GameDump key={game} />
      <GameAssets />
      <DevPanel key={beckon} />
      <UpdateCheck />
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

/** The dev panel toggle. Beckoned, it scrolls into view and pulses. */
function DevPanel() {
  const [checked] = useConfig.usePath("config.dev_panel");
  const [beckon] = useShell.usePath("beckon");
  const beckoned = beckon > 0;
  const save = useRequestState();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (beckoned)
      ref.current?.scrollIntoView?.({ behavior: "smooth", block: "center" });
  }, [beckoned]);

  return (
    <div ref={ref} className={cn("rounded-lg", beckoned && "animate-beckon")}>
      <SettingsSection>
        <Row
          id="dev-panel"
          label="Dev panel"
          description="Seeds you install carry the dev panel's hook, and the sidebar's wrench opens the panel. Takes effect at the next install, then restart the game."
          under={<InlineError error={save.error} />}
        >
          <Switch
            id="dev-panel"
            checked={checked}
            disabled={save.busy}
            onCheckedChange={(on) =>
              save.run(() => saveConfig({ dev_panel: on }))
            }
          />
        </Row>
      </SettingsSection>
    </div>
  );
}

/** Whether the app asks GitHub for a newer release. */
function UpdateCheck() {
  const [off] = useConfig.usePath("config.no_update_check");
  const save = useRequestState();
  return (
    <SettingsSection>
      <Row
        id="update-check"
        label="Check for updates"
        description="Asks GitHub for the latest release at launch and every 6 hours, and the sidebar says when a newer one is out. Off, the app never contacts GitHub."
        under={<InlineError error={save.error} />}
      >
        <Switch
          id="update-check"
          checked={!off}
          disabled={save.busy}
          onCheckedChange={(on) =>
            save.run(() => saveConfig({ no_update_check: !on }))
          }
        />
      </Row>
    </SettingsSection>
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
