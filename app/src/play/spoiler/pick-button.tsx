import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";
import { useSpoiler } from "@/play/spoiler/spoiler-context";

/** Props for {@link PickButton}: a button's, but for what picking sets. */
interface PickButtonProps
  extends Omit<ComponentProps<"button">, "type" | "onClick"> {
  /** The check it picks. */
  hash: string;
  /** Style the picked state with `data-picked:`. */
  className?: string;
}

/** A button that picks a check, marked `data-picked` while it's the one.
 * Pressing the picked one again clears the pick. */
export function PickButton({ hash, className, ...rest }: PickButtonProps) {
  const { picked, pick } = useSpoiler();
  return (
    <button
      {...rest}
      type="button"
      aria-pressed={picked === hash}
      data-picked={picked === hash || undefined}
      className={cn("cursor-pointer", className)}
      onClick={() => pick(picked === hash ? undefined : hash)}
    />
  );
}
