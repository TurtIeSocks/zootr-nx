import { CircleAlert, CircleOff, Dices } from "lucide-react";
import { Callout, InlineError } from "@/components/callout";
import { CardHeader } from "@/components/card";
import { FixLink } from "@/components/fix-link";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { CLOSE_GAME } from "@/lib/config";
import { seedOrCode } from "@/lib/format";
import { BuildOverview } from "@/play/build-overview";
import { generateKey } from "@/play/generate";
import { GenerateButton } from "@/play/generate-button";
import { GenerateSteps } from "@/play/generate-steps";
import { uninstall, useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

/** The Standard folder: generates and installs a seed from the current
 * settings, or from a share code's own. */
export function NewSeed() {
  const [error] = useSeeds.usePath("error");
  return (
    <>
      <CardHeader
        title="New Seed"
        help="seed"
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
        <BuildOverview />
        <Blocked />
        <InlineError error={error} />
      </div>
    </>
  );
}

/** The seed to generate with Roll inside it, Uninstall once something is
 * installed, and Generate. */
function SeedRow() {
  return (
    <div className="flex flex-wrap items-center gap-2">
      <SeedInput />
      <UninstallButton />
      <GenerateButton />
    </div>
  );
}

/** The seed or share code to generate from, blank to roll one, with Roll
 * at its trailing edge. */
function SeedInput() {
  const [seed, setSeed] = useUi.usePath("newSeed.seed");
  return (
    <div className="relative w-[280px] max-w-full">
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
    <Tip tip="Roll a seed">
      <Button
        variant="ghost"
        size="icon-sm"
        aria-label="Roll a seed"
        className="-translate-y-1/2 absolute top-1/2 right-1.5"
        onClick={() =>
          useUi.setPath(
            "newSeed.seed",
            String(Math.floor(Math.random() * Number.MAX_SAFE_INTEGER)),
          )
        }
      >
        <Dices />
      </Button>
    </Tip>
  );
}

/** Takes the mod off, once one is installed and the game is closed. */
function UninstallButton() {
  const installed = useSeeds((s) => s.history.installed !== null);
  const [busy] = useSeeds.usePath("busy");
  const [running] = useSeeds.usePath("running");
  if (!installed) return null;
  return (
    <TipButton
      label="Uninstall"
      tip={running ? CLOSE_GAME : "Take the mod off so the game plays vanilla"}
      variant="secondary"
      size="icon-lg"
      disabled={busy || running}
      onClick={uninstall}
    >
      <CircleOff />
    </TipButton>
  );
}

/** Why nothing can be installed, while no job runs. */
function Blocked() {
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  if (!blocked || busy) return null;
  return (
    <Callout tone="gold" icon={CircleAlert}>
      <FixLink why={blocked} />
    </Callout>
  );
}
