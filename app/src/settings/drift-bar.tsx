import { Dices, Loader2 } from "lucide-react";
import { ChangeList } from "@/components/change-list";
import { FloatingBar } from "@/components/floating-bar";
import { Help } from "@/components/help";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { WhyButton } from "@/components/why-button";
import { generate, useDriftFrom, useSeeds } from "@/stores/seeds";
import { fix } from "@/stores/shell";

/** Floats at the foot of a settings page while the settings differ from the
 * installed seed's (the defaults with none installed), with what differs
 * and a way to make a seed from them. */
export function DriftBar() {
  const [changes] = useSeeds.usePath("drift");
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  const from = useDriftFrom();
  const n = changes.length;

  return (
    <FloatingBar
      message={
        <p>
          <b className="font-semibold">
            {n} setting{n === 1 ? "" : "s"}
          </b>{" "}
          {n === 1 ? "differs" : "differ"} from {from} <Help topic="drift" />
        </p>
      }
    >
      <Popover>
        <PopoverTrigger asChild>
          <Button variant="ghost">Review</Button>
        </PopoverTrigger>
        <PopoverContent side="top" className="text-[13px]">
          <p className="mb-1 font-medium">Changed from {from}</p>
          <ChangeList changes={changes} />
        </PopoverContent>
      </Popover>
      <WhyButton
        disabled={busy}
        why={blocked}
        onWhy={() => fix(blocked)}
        onClick={() => generate(null)}
      >
        {busy ? <Loader2 className="animate-spin" /> : <Dices />}
        Generate & install
      </WhyButton>
    </FloatingBar>
  );
}
