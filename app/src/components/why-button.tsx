import type { ComponentProps, ReactNode } from "react";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";

/** Props for {@link WhyButton}. */
export interface WhyButtonProps extends ComponentProps<typeof Button> {
  /** Why it can't act now. Set, it stays hoverable (a disabled button gets
   * no pointer events), its click does nothing but `onWhy`, and its tip
   * says why. */
  why?: ReactNode;
  /** Its tip while it can act. */
  tip?: ReactNode;
  /** Runs on a click while it can't act: opening the page that fixes it. */
  onWhy?: () => void;
}

/** A button that says why it can't act, rather than failing on click. */
export function WhyButton({
  why,
  tip,
  onWhy,
  onClick,
  ...props
}: WhyButtonProps) {
  const blocked = why != null && why !== false && why !== "";
  return (
    <Tip tip={blocked ? why : tip}>
      <Button
        {...props}
        aria-disabled={blocked || undefined}
        onClick={blocked ? onWhy : onClick}
      />
    </Tip>
  );
}
