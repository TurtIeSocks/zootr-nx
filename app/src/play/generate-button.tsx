import {
  ChevronDownIcon,
  CircleStop,
  ListPlus,
  Loader2,
  type LucideIcon,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ButtonGroup,
  ButtonGroupSeparator,
} from "@/components/ui/button-group";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuShortcut,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { WhyButton } from "@/components/why-button";
import { MOD } from "@/lib/platform";
import { generateTyped } from "@/play/generate";
import { stepName, stopGenerate, useSeeds } from "@/stores/seeds";
import { fix } from "@/stores/shell";

/** Generate & install, with Generate only behind its chevron, the same
 * shape as Play. While a seed generates, it shows the step and turns into
 * Stop on hover. */
export function GenerateButton() {
  const [generating] = useSeeds.usePath("generating");
  return (
    <ButtonGroup className="ml-auto">
      {generating ? <Progress /> : <Install />}
      <ButtonGroupSeparator className="bg-on-primary/20" />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            size="lg"
            className="w-9 px-0"
            aria-label="More ways to generate"
            disabled={generating}
          >
            <ChevronDownIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-[300px]">
          <Way
            install
            icon={Sparkles}
            label="Generate & install"
            line="Builds it and installs it to every target"
            keys={`${MOD}↵`}
          />
          <Way
            install={false}
            icon={ListPlus}
            label="Generate only"
            line="Adds it to History without installing"
            keys={`⇧${MOD}↵`}
          />
        </DropdownMenuContent>
      </DropdownMenu>
    </ButtonGroup>
  );
}

/** The main button: generates and installs. Blocked, it stays hoverable
 * and its tooltip says why. */
function Install() {
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  return (
    <WhyButton
      size="lg"
      className="min-w-[228px]"
      disabled={busy}
      why={blocked}
      onWhy={() => fix(blocked)}
      onClick={() => generateTyped(true, blocked)}
    >
      {busy ? <Loader2 className="animate-spin" /> : <Sparkles />}
      Generate & install
    </WhyButton>
  );
}

/** Props for {@link Way}. */
interface WayProps {
  install: boolean;
  icon: LucideIcon;
  label: string;
  line: string;
  keys: string;
}

/** One way to generate, in the chevron's menu. Blocked, its line says
 * why. */
function Way({ install, icon: Icon, label, line, keys }: WayProps) {
  const [why] = useSeeds.usePath(install ? "blocked" : "unbuildable");
  return (
    <DropdownMenuItem
      className="h-auto items-start py-2"
      disabled={!!why}
      onSelect={() => generateTyped(install, why)}
    >
      <Icon className="mt-0.5" />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-ink">{label}</span>
        <span className="text-ink-muted text-xs">{why || line}</span>
      </span>
      <DropdownMenuShortcut>{keys}</DropdownMenuShortcut>
    </DropdownMenuItem>
  );
}

/** The running job: its step and count over a fill that grows as steps
 * end. Hover or focus shows Stop, which a click asks for. */
function Progress() {
  const [steps] = useSeeds.usePath("steps");
  const [stopping] = useSeeds.usePath("stopping");
  const rows = steps?.rows ?? [];
  const total = Math.max(steps?.total ?? 0, rows.length, 1);
  const done = rows.filter((r) => r.secs !== null).length;
  const now = rows.at(-1);
  return (
    <Button
      size="lg"
      className="group/gen relative isolate min-w-[228px] justify-start overflow-hidden"
      aria-label={stopping ? "Stopping" : "Stop"}
      disabled={stopping}
      onClick={stopGenerate}
    >
      <span
        aria-hidden
        className="-z-1 absolute inset-y-0 left-0 bg-primary-hover transition-[width] duration-(--duration-slow) ease-out"
        style={{ width: `${(done / total) * 100}%` }}
      />
      {stopping ? (
        <>
          <Loader2 className="animate-spin" />
          Stopping…
        </>
      ) : (
        <>
          <span className="inline-flex items-center gap-2 group-focus-visible/gen:hidden group-hover/gen:hidden">
            <Loader2 className="animate-spin" />
            {now ? `${stepName(now.step)}…` : "Starting…"}
          </span>
          <span className="hidden items-center gap-2 group-focus-visible/gen:inline-flex group-hover/gen:inline-flex">
            <CircleStop />
            Stop
          </span>
        </>
      )}
      <span className="ml-auto font-[450] tabular-nums opacity-80">
        {Math.min(done + 1, total)} / {total}
      </span>
    </Button>
  );
}
