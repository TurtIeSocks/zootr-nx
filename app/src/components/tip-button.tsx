import type { ComponentProps, ReactNode } from "react";
import type { Button } from "@/components/ui/button";
import { WhyButton } from "@/components/why-button";

/** Props for {@link TipButton}. */
export interface TipButtonProps
  extends Pick<
    ComponentProps<typeof Button>,
    "variant" | "size" | "className"
  > {
  /** Read by screen readers and tests. */
  label: string;
  tip: ReactNode;
  disabled?: boolean;
  /** Makes it a toggle, pressed or not. */
  pressed?: boolean;
  onClick: () => void;
  onBlur?: () => void;
  children: ReactNode;
}

/** An icon button with a tooltip. Disabled stays hoverable (a disabled
 * button gets no pointer events), so the tooltip can say why. */
export function TipButton({
  label,
  tip,
  disabled,
  pressed,
  onClick,
  onBlur,
  children,
  variant = "secondary",
  size = "icon-sm",
  className,
}: TipButtonProps) {
  return (
    <WhyButton
      variant={variant}
      size={size}
      className={className}
      aria-label={label}
      aria-pressed={pressed}
      why={disabled ? tip : undefined}
      tip={tip}
      onClick={onClick}
      onBlur={onBlur}
    >
      {children}
    </WhyButton>
  );
}
