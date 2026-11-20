import { ChevronDownIcon, Play } from "lucide-react";
import { useState } from "react";
import { commands } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  ButtonGroup,
  ButtonGroupSeparator,
} from "@/components/ui/button-group";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { WhyButton } from "@/components/why-button";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { RebuildDialog } from "@/play/rebuild-dialog";
import { saveConfig, useConfig } from "@/stores/config";
import { rebuild, stale, useBuild, useTargetRunning } from "@/stores/seeds";

/** Play the picked target, with the other targets a click away when there
 * are others. A game whose build lacks the installed hook patch asks
 * first. Runs in the request around it, which shows why any failed. */
export function PlayButtonGroup() {
  const play = useRequest();
  const [targets] = useConfig.usePath("config.targets");
  const [choice] = useConfig.usePath("config.picked");
  const picked = choice ?? targets[0]?.name;
  const build = useBuild(picked);
  const running = useTargetRunning(picked);
  const [asking, setAsking] = useState<string>();
  const start = (name: string) => play.run(() => call(commands.play(name)));

  return (
    <ButtonGroup>
      <WhyButton
        size="lg"
        disabled={!picked || play.busy}
        why={running && `${picked} is already running`}
        onClick={() => {
          if (!picked) return;
          if (stale(build)) setAsking(picked);
          else start(picked);
        }}
      >
        <Play /> Play {picked}
      </WhyButton>
      {targets.length > 1 && (
        <>
          <ButtonGroupSeparator className="bg-on-primary/20" />
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="lg" className="w-8 px-0" aria-label="Pick a target">
                <ChevronDownIcon />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuRadioGroup
                value={picked}
                onValueChange={(name) =>
                  play.run(() => saveConfig({ picked: name }))
                }
              >
                {targets.map((t) => (
                  <DropdownMenuRadioItem key={t.name} value={t.name}>
                    {t.name}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </>
      )}
      <RebuildDialog
        name={asking}
        build={build}
        onClose={() => setAsking(undefined)}
        onPlay={() => asking && start(asking)}
        onRebuild={() =>
          asking &&
          play.run(async () => {
            // A rebuild that ends without the build current throws.
            if (await rebuild(asking)) await call(commands.play(asking));
          })
        }
      />
    </ButtonGroup>
  );
}
