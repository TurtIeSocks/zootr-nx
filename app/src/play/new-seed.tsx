import {
  CircleAlert,
  CircleOff,
  Dices,
  FlaskConical,
  type LucideIcon,
  Pin,
} from "lucide-react";
import type { ReactNode } from "react";
import { Callout, InlineError } from "@/components/callout";
import { CardHeader } from "@/components/card";
import { linkClass } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { testSummary } from "@/lib/config";
import { count, seedOrCode } from "@/lib/format";
import { savedAs } from "@/lib/plando";
import { summary } from "@/lib/settings";
import { generateKey } from "@/play/generate";
import { GenerateButton } from "@/play/generate-button";
import { GenerateSteps } from "@/play/generate-steps";
import { SettingsChanged } from "@/play/settings-changed";
import { useConfig } from "@/stores/config";
import { uninstall, useSeeds } from "@/stores/seeds";
import { openSettings } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** The Standard folder: generates and installs a seed from the current
 * settings, or from a share code's own. */
export function NewSeed() {
  const [error] = useSeeds.usePath("error");
  return (
    <>
      <CardHeader
        title="New seed"
        description="Built from your current settings, or a share code's, and installed to every target"
      />
      {/* ⌘↵ and ⇧⌘↵ work from anywhere in the folder. */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: keys bubble up from its controls */}
      <div
        className="flex flex-col gap-3 px-4 pt-2 pb-4"
        onKeyDown={generateKey}
      >
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
    </>
  );
}

/** The seed to generate with Roll inside it, Vanilla once something is
 * installed, and Generate. */
function SeedRow() {
  return (
    <div className="flex items-center gap-2">
      <SeedInput />
      <VanillaButton />
      <GenerateButton />
    </div>
  );
}

/** The seed or share code to generate from, blank to roll one, with Roll
 * at its trailing edge. */
function SeedInput() {
  const [seed, setSeed] = useUi.usePath("newSeed.seed");
  return (
    <div className="relative w-[280px]">
      <Input
        className="h-10 pr-10 font-mono text-[15px]"
        placeholder="Random, or a share code"
        aria-label="Seed or share code"
        value={seed}
        onChange={(e) => setSeed(seedOrCode(e.target.value))}
      />
      <RollButton />
    </div>
  );
}

/** Rolls a random seed into the input. */
function RollButton() {
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className="-translate-y-1/2 absolute top-1/2 right-1.5"
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
