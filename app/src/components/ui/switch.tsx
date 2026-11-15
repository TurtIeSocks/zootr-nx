import { Switch as SwitchPrimitive } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

/** 36 by 20 with a 14px thumb that springs across and stretches while
 * pressed. */
function Switch({
  className,
  ...props
}: React.ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      className={cn(
        "peer group/switch relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full bg-surface-sunken shadow-[inset_0_0_0_1px_var(--control-border)] outline-none transition-[background-color,box-shadow] duration-(--duration-base) ease-out disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=checked]:shadow-none",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className="pointer-events-none absolute top-[3px] left-[3px] block h-3.5 w-3.5 rounded-full bg-control-border shadow-xs transition-[transform,width,background-color] duration-(--duration-base) ease-spring group-active/switch:w-[17px] data-[state=checked]:translate-x-4 data-[state=checked]:bg-thumb data-[state=checked]:shadow-sm group-active/switch:data-[state=checked]:translate-x-[13px]"
      />
    </SwitchPrimitive.Root>
  );
}

export { Switch };
