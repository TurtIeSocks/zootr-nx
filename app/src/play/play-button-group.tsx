import { ChevronDownIcon, Play } from "lucide-react";
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
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { saveConfig, useConfig } from "@/stores/config";

/** Play the picked target, with the other targets a click away. Runs in
 * the request around it, which shows why either failed. */
export function PlayButtonGroup() {
  const play = useRequest();
  const [targets] = useConfig.usePath("config.targets");
  const [choice] = useConfig.usePath("config.picked");
  const picked = choice ?? targets[0]?.name;

  return (
    <ButtonGroup>
      <Button
        size="lg"
        disabled={!picked || play.busy}
        onClick={() => picked && play.run(() => call(commands.play(picked)))}
      >
        <Play /> Play {picked}
      </Button>
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
    </ButtonGroup>
  );
}
