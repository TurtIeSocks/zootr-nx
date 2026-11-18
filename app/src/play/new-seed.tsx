import { open as pickFile } from "@tauri-apps/plugin-dialog";
import {
  CircleAlert,
  CircleOff,
  CircleStop,
  Dices,
  FlaskConical,
  FolderOpen,
  Loader2,
  type LucideIcon,
  Pin,
  ScrollText,
  Sparkles,
} from "lucide-react";
import type { ReactNode } from "react";
import { Callout, InlineError } from "@/components/callout";
import { Card } from "@/components/card";
import { linkClass } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { testSummary } from "@/lib/config";
import { count, seedOrCode } from "@/lib/format";
import { savedAs } from "@/lib/plando";
import { summary } from "@/lib/settings";
import { GenerateSteps } from "@/play/generate-steps";
import { SettingsChanged } from "@/play/settings-changed";
import { useConfig } from "@/stores/config";
import {
  generate,
  generateAp,
  stopGenerate,
  uninstall,
  useSeeds,
} from "@/stores/seeds";
import { openSettings } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** Generates and installs a seed from the current settings, or from a
 * share code's own. */
export function NewSeed() {
  const [error] = useSeeds.usePath("error");
  return (
    <Card
      title="New seed"
      description="Built from your current settings, or a share code's, and installed to every target"
      className="shrink-0"
    >
      <div className="flex flex-col gap-3 px-4 pt-2 pb-4">
        <SeedRow />
        <GenerateSteps />
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[13px] text-ink-muted">
          <SettingsSummary />
          <button
            type="button"
            className={linkClass}
            onClick={() => openSettings()}
          >
            edit
          </button>
          <Drift />
        </p>
        <Plando />
        <TestSetup />
        <Blocked />
        <InlineError error={error} />
      </div>
    </Card>
  );
}

/** The seed to generate, a roll, Vanilla once something is installed, and
 * Generate. */
function SeedRow() {
  return (
    <div className="flex items-center gap-2">
      <SeedInput />
      <RollButton />
      <VanillaButton />
      <OpenApButton />
      <GenerateButton />
    </div>
  );
}

/** The seed or share code to generate from; blank rolls one. */
function SeedInput() {
  const [seed, setSeed] = useUi.usePath("newSeed.seed");
  return (
    <Input
      className="h-10 w-60 font-mono text-[15px]"
      placeholder="Random, or a share code"
      aria-label="Seed or share code"
      value={seed}
      onChange={(e) => setSeed(seedOrCode(e.target.value))}
    />
  );
}

/** Rolls a random seed into the input. */
function RollButton() {
  return (
    <Button
      variant="secondary"
      size="icon-lg"
      title="Roll a seed"
      onClick={() =>
        useUi.setPath(
          "newSeed.seed",
          String(Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)),
        )
      }
    >
      <Dices />
    </Button>
  );
}

/** Takes the mod off, once one is installed and the game is closed. */
function VanillaButton() {
  const installed = useSeeds((s) => s.history.installed !== null);
  const [busy] = useSeeds.usePath("busy");
  const [running] = useSeeds.usePath("running");
  if (!installed) return null;
  return (
    <TipButton
      label="Vanilla"
      tip={
        running
          ? "Close the game first"
          : "Take the mod off to play the game vanilla"
      }
      variant="secondary"
      size="icon-lg"
      disabled={busy || running}
      onClick={uninstall}
    >
      <CircleOff />
    </TipButton>
  );
}

/** Installs an Archipelago seed from the `.apznx` picked. The file brings
 * its own settings, so the test setup and plando don't apply. */
function OpenApButton() {
  const [busy] = useSeeds.usePath("busy");
  const [blocked] = useSeeds.usePath("blocked");
  const open = async () => {
    const file = await pickFile({
      filters: [{ name: "Archipelago seed", extensions: ["apznx"] }],
    });
    if (file) await generateAp(file);
  };
  return (
    <TipButton
      label="Open AP seed"
      tip={blocked || "Install your slot of an Archipelago multiworld"}
      variant="secondary"
      size="icon-lg"
      disabled={busy || !!blocked}
      onClick={open}
    >
      <FolderOpen />
    </TipButton>
  );
}

/** Generate only, which adds the seed to History, and Generate & install.
 * While a seed generates, they're Stop. */
export function GenerateButton() {
  const [blocked] = useSeeds.usePath("blocked");
  const [unbuildable] = useSeeds.usePath("unbuildable");
  const [generating] = useSeeds.usePath("generating");
  const [stopping] = useSeeds.usePath("stopping");
  if (generating) {
    return (
      <Button
        size="lg"
        variant="secondary"
        className="ml-auto"
        disabled={stopping}
        onClick={stopGenerate}
      >
        {stopping ? <Loader2 className="animate-spin" /> : <CircleStop />}
        {stopping ? "Stopping" : "Stop"}
      </Button>
    );
  }
  return (
    <>
      <GenerateAction
        install={false}
        blocked={unbuildable}
        icon={ScrollText}
        className="ml-auto"
        variant="secondary"
      >
        Generate only
      </GenerateAction>
      <GenerateAction install blocked={blocked} icon={Sparkles}>
        Generate & install
      </GenerateAction>
    </>
  );
}

/** Props for {@link GenerateAction}. */
interface GenerateActionProps {
  /** Install the seed, else only add it to History. */
  install: boolean;
  /** Why it can't run, or "". */
  blocked: string;
  icon: LucideIcon;
  className?: string;
  variant?: "secondary";
  children: ReactNode;
}

/** One way to generate from the typed seed, read on click. Blocked, it
 * stays hoverable and its tooltip says why. */
function GenerateAction({
  install,
  blocked,
  icon: Icon,
  className,
  variant,
  children,
}: GenerateActionProps) {
  const [busy] = useSeeds.usePath("busy");
  return (
    <Tooltip open={blocked ? undefined : false}>
      <TooltipTrigger asChild>
        <Button
          size="lg"
          variant={variant}
          className={className}
          disabled={busy}
          aria-disabled={!!blocked || undefined}
          onClick={
            blocked
              ? undefined
              : () => generate(useUi.getPath("newSeed.seed") || null, install)
          }
        >
          {busy ? <Loader2 className="animate-spin" /> : <Icon />}
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{blocked}</TooltipContent>
    </Tooltip>
  );
}

/** The current settings in a line. */
function SettingsSummary() {
  const [schema] = useConfig.usePath("schema");
  const [settings] = useConfig.usePath("config.settings");
  return <span>{summary(schema, settings)}</span>;
}

/** Flags settings changed since the installed seed. */
function Drift() {
  const [drift] = useSeeds.usePath("drift");
  return drift.length > 0 && <SettingsChanged changes={drift} />;
}

/** Props for {@link EditCallout}. */
interface EditCalloutProps {
  icon: LucideIcon;
  /** What its edit link opens, for the link's name. */
  what: string;
  /** The Settings page the link opens. */
  section: string;
  children: ReactNode;
}

/** Something Generate builds with, and a link to edit it. */
function EditCallout({ icon, what, section, children }: EditCalloutProps) {
  return (
    <Callout
      tone="info"
      icon={icon}
      action={
        <button
          type="button"
          className={linkClass}
          aria-label={`Edit ${what}`}
          onClick={() => openSettings(section)}
        >
          edit
        </button>
      }
    >
      <span>{children}</span>
    </Callout>
  );
}

/** The stored test setup, while one is on. */
function TestSetup() {
  const [on] = useConfig.usePath("testOn");
  const [test] = useConfig.usePath("config.test");
  if (!on) return null;
  return (
    <EditCallout icon={FlaskConical} what="test setup" section="test">
      {testSummary(test)}
    </EditCallout>
  );
}

/** The plando draft, while it pins anything. */
function Plando() {
  const pins = useConfig((s) => Object.keys(s.config.place).length);
  const name = useConfig((s) => savedAs(s.config));
  if (!pins) return null;
  return (
    <EditCallout icon={Pin} what="plando" section="plando">
      Plando{name && ` ${name}`}: {count(pins, "check")} pinned
    </EditCallout>
  );
}

/** Why nothing can be installed, while no job runs. */
function Blocked() {
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  if (!blocked || busy) return null;
  return (
    <Callout tone="gold" icon={CircleAlert}>
      {blocked}
    </Callout>
  );
}
