import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { useArmed } from "@/play/use-armed";

/** Props for {@link ArmedButton}: a button's, but for the handlers
 * {@link useArmed} owns. */
interface ArmedButtonProps
  extends Omit<
    ComponentProps<typeof Button>,
    keyof ReturnType<typeof useArmed> | "children"
  > {
  /** What the second click does, as "Really <verb>?" asks it. */
  verb: string;
  /** Runs on the second click. */
  onAct: () => void;
  /** The button as it rests. */
  children: ReactNode;
}

/** A destructive button that asks for a second click: the first arms it,
 * red and reading "Really <verb>?", the second acts, and moving away
 * disarms. An icon button grows to fit the question. */
export function ArmedButton({
  verb,
  onAct,
  children,
  variant = "ghost",
  size = "default",
  ...rest
}: ArmedButtonProps) {
  const { armed, ...handlers } = useArmed(onAct);
  return (
    <Button
      variant={armed ? "destructive" : variant}
      size={armed && size?.startsWith("icon") ? "sm" : size}
      {...rest}
      {...handlers}
    >
      {armed ? `Really ${verb}?` : children}
    </Button>
  );
}
