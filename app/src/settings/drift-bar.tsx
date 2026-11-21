import { Dices, Loader2 } from "lucide-react";
import { DriftReview } from "@/components/drift-review";
import { FloatingBar } from "@/components/floating-bar";
import { Help } from "@/components/help";
import { Button } from "@/components/ui/button";
import { WhyButton } from "@/components/why-button";
import { generate, useDriftWords, useSeeds } from "@/stores/seeds";
import { fix, useShell } from "@/stores/shell";

/** Floats at the foot of every settings page while the settings differ
 * from the installed seed's (the defaults with none installed), with what
 * differs and a way to make a seed from them. A page's own unsaved bar
 * takes its place while it shows. */
export function DriftBar() {
  const drifted = useSeeds((s) => s.drift.length > 0);
  const unsaved = useShell((s) => (s.unsaved ?? 0) > 0);
  return drifted && !unsaved && <Bar />;
}

/** The bar itself: the count, Review and Generate & Install. */
function Bar() {
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
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
      <WhyButton
        disabled={busy}
        why={blocked}
        onWhy={() => fix(blocked)}
        onClick={() => generate(null)}
      >
        {busy ? <Loader2 className="animate-spin" /> : <Dices />}
        Generate & Install
      </WhyButton>
    </FloatingBar>
  );
}
