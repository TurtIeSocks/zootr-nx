import { Shuffle } from "lucide-react";
import type { ComponentProps } from "react";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";

/** Props for {@link RandomizeButton}. */
interface RandomizeButtonProps
  extends Pick<ComponentProps<typeof Button>, "disabled" | "onClick"> {
  /** What it randomizes. */
  tip: string;
}

/** A pane's Randomize, beside its resets. */
export function RandomizeButton({ tip, ...props }: RandomizeButtonProps) {
  return (
    <Tip tip={tip}>
      <Button variant="ghost" {...props}>
        <Shuffle />
        Randomize
      </Button>
    </Tip>
  );
}
