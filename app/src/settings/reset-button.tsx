import {
  ChevronDownIcon,
  History,
  type LucideIcon,
  RotateCcw,
} from "lucide-react";
import type { SettingDef } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
  ButtonGroup,
  ButtonGroupSeparator,
} from "@/components/ui/button-group";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { WhyButton } from "@/components/why-button";
import { useRequest } from "@/lib/request";
import { defaults, isChanged, type Patch } from "@/lib/settings";
import { changeSettings, useConfig } from "@/stores/config";
import { useDriftBase, useSeeds } from "@/stores/seeds";
import { type ResetTo, useUi } from "@/stores/ui";

/** Props for {@link ResetButton}. */
export interface ResetButtonProps {
  /** The page's settings. */
  defs: SettingDef[];
}

/** One way to reset: what it says, its patch, and why it would do
 * nothing. */
interface Reset {
  icon: LucideIcon;
  label: string;
  line: string;
  patch: Patch;
  why: string | false;
}

/** A page's settings back to their defaults or to the last seed's, the
 * one used last leading and the other a click away in its menu. The last
 * seed is the one drift measures from. */
export function ResetButton({ defs }: ResetButtonProps) {
  const { run } = useRequest();
  const [lead, setLead] = useUi.usePath("settings.reset");
  const changed = useConfig((s) =>
    defs.some((d) => isChanged(d, s.config.settings)),
  );
  const base = useDriftBase();
  const drifted = useSeeds((s) =>
    s.drift.some((c) => defs.some((d) => d.id === c.def.id)),
  );
  const resets: Record<ResetTo, Reset> = {
    defaults: {
      icon: RotateCcw,
      label: "Reset to defaults",
      line: "Every setting here back to its default",
      patch: defaults(defs),
      why: !changed && "Already the defaults",
    },
    seed: {
      icon: History,
      label: "Reset to last seed",
      line: "Every setting here as the last seed had it",
      patch: Object.fromEntries(
        defs.map((d) => [d.id, base?.settings[d.id] as Patch[string]]),
      ),
      why: base
        ? !drifted && "Already the last seed's settings"
        : "No seed built yet",
    },
  };
  const use = (to: ResetTo) => {
    setLead(to);
    run(() => changeSettings(resets[to].patch));
  };
  const led = resets[lead];
  return (
    <ButtonGroup>
      <WhyButton variant="ghost" why={led.why} onClick={() => use(lead)}>
        <led.icon />
        {led.label}
      </WhyButton>
      <ButtonGroupSeparator />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            className="w-8 px-0"
            aria-label="More ways to reset"
          >
            <ChevronDownIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-[300px]">
          {(Object.keys(resets) as ResetTo[]).map((to) => {
            const { icon: Icon, label, line, why } = resets[to];
            return (
              <DropdownMenuItem
                key={to}
                className="h-auto items-start py-2"
                disabled={why !== false}
                onSelect={() => use(to)}
              >
                <Icon className="mt-0.5" />
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="text-ink">{label}</span>
                  <span className="text-ink-muted text-xs">{why || line}</span>
                </span>
              </DropdownMenuItem>
            );
          })}
        </DropdownMenuContent>
      </DropdownMenu>
    </ButtonGroup>
  );
}
