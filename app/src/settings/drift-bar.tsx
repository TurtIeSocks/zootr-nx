import { Dices, Loader2, Paintbrush } from "lucide-react";
import { DriftReview } from "@/components/drift-review";
import { FloatingBar } from "@/components/floating-bar";
import { Help } from "@/components/help";
import { SplitButton, type SplitOption } from "@/components/split-button";
import { Button } from "@/components/ui/button";
import { WhyButton } from "@/components/why-button";
import { PANE_OF } from "@/lib/drift";
import {
  applyCosmetics,
  generate,
  useDriftBase,
  useDriftCount,
  useDriftWords,
  useSeeds,
} from "@/stores/seeds";
import { fix, useShell } from "@/stores/shell";

/** Floats at the foot of every settings page while the settings or the
 * panes' drafts differ from the installed seed's (the defaults with none
 * installed), with what differs and a way to make a seed from them. A
 * page's own unsaved bar takes its place while it shows. */
export function DriftBar() {
  const drifted = useDriftCount() > 0;
  const unsaved = useShell((s) => (s.unsaved ?? 0) > 0);
  return drifted && !unsaved && <Bar />;
}

/** Whether only colors and music drifted from the installed seed: then
 * painting it again is enough, with no new shuffle. */
function useCosmeticOnly() {
  const base = useDriftBase();
  const installed = useSeeds((s) => s.history.installed);
  return useSeeds(
    (s) =>
      !!installed &&
      base?.id === installed.id &&
      s.drift.length === 0 &&
      s.paneDrift.every((c) => PANE_OF[c.kind] !== "plando"),
  );
}

/** The bar itself: the count, Review, and Generate & Install, or Apply to
 * Installed Seed when only colors and music changed. */
function Bar() {
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  const why = busy ? "Already running" : blocked;
  const spinner = busy ? <Loader2 className="animate-spin" /> : undefined;
  const make: SplitOption = {
    icon: Dices,
    label: "Generate & Install",
    line: "A new seed from these settings, colors and music",
    why: why || false,
    onWhy: () => fix(blocked),
    act: () => generate(null),
  };
  const paint: SplitOption = {
    icon: Paintbrush,
    label: "Apply to Installed Seed",
    line: "The installed seed in these colors and music, same shuffle",
    why: why || false,
    onWhy: () => fix(blocked),
    act: () => applyCosmetics(),
  };
  return (
    <FloatingBar
      message={
        <p>
          <b className="font-semibold">{useDriftWords()}</b>{" "}
          <Help topic="drift" />
        </p>
      }
    >
      <DriftReview side="top">
        <Button variant="ghost">Review</Button>
      </DriftReview>
      {useCosmeticOnly() ? (
        <SplitButton
          options={[paint, make]}
          lead={paint}
          more="More ways to use the changes"
          icon={spinner}
        />
      ) : (
        <WhyButton
          disabled={busy}
          why={blocked}
          onWhy={() => fix(blocked)}
          onClick={make.act}
        >
          {spinner ?? <Dices />}
          Generate & Install
        </WhyButton>
      )}
    </FloatingBar>
  );
}
