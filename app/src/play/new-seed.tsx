import {
  CircleAlert,
  CircleOff,
  Dices,
  FlaskConical,
  Loader2,
  type LucideIcon,
  Pin,
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
import { SettingsChanged } from "@/play/settings-changed";
import { useConfig } from "@/stores/config";
import { generate, uninstall, useSeeds } from "@/stores/seeds";
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

/** Generates from the typed seed, read on click. Blocked, it stays
 * hoverable and its tooltip says why. */
function GenerateButton() {
  const [busy] = useSeeds.usePath("busy");
  const [blocked] = useSeeds.usePath("blocked");
  return (
    <Tooltip open={blocked ? undefined : false}>
      <TooltipTrigger asChild>
        <Button
          size="lg"
          className="ml-auto"
          disabled={busy}
          aria-disabled={!!blocked || undefined}
          onClick={
            blocked
              ? undefined
              : () => generate(useUi.getPath("newSeed.seed") || null)
          }
        >
          {busy ? <Loader2 className="animate-spin" /> : <Sparkles />}
          Generate & install
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
