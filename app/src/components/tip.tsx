import type { ReactElement, ReactNode } from "react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";

/** How long the pointer rests before a tip shows. A native `title`
 * waits as long as the OS likes, which is why the app has none. */
const TIP_MS = 150;

/** Props for {@link Tip}. */
export interface TipProps {
  /** Nothing shows while it's empty. */
  tip: ReactNode;
  /** Wraps the child in a span: a disabled control gets no pointer
   * events, so the tip couldn't say why it's disabled. */
  wrap?: boolean;
  /** One element that takes a ref and spreads its props. */
  children: ReactElement;
}

/** The app's one hover hint. It says what the element doesn't already
 * show (an icon's meaning, a cut-off text in full, a shortcut, why it's
 * disabled), never its own label again. */
export function Tip({ tip, wrap, children }: TipProps) {
  if (tip == null || tip === "") return children;
  return (
    <Tooltip delayDuration={TIP_MS}>
      <TooltipTrigger asChild>
        {wrap ? <span className="inline-flex">{children}</span> : children}
      </TooltipTrigger>
      <TooltipContent className="whitespace-pre-line">{tip}</TooltipContent>
    </Tooltip>
  );
}
