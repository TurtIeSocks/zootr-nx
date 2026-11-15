import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex h-5 w-fit shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded-full px-2 font-[550] text-[11.5px] leading-none [&>svg]:pointer-events-none [&>svg]:size-3 [&>svg]:stroke-2",
  {
    variants: {
      variant: {
        default:
          "bg-surface-sunken text-ink-muted shadow-[inset_0_0_0_1px_var(--line)]",
        green: "bg-primary-soft text-primary-ink",
        info: "bg-info-soft text-info-ink",
        gold: "bg-gold-soft text-gold-ink",
        danger: "bg-danger-soft text-danger-ink",
      },
      shape: {
        pill: "",
        square: "h-4.5 rounded-xs px-1.5",
      },
    },
    defaultVariants: {
      variant: "default",
      shape: "pill",
    },
  },
);

function Badge({
  className,
  variant = "default",
  shape = "pill",
  asChild = false,
  ...props
}: React.ComponentProps<"span"> &
  VariantProps<typeof badgeVariants> & { asChild?: boolean }) {
  const Comp = asChild ? Slot.Root : "span";

  return (
    <Comp
      data-slot="badge"
      data-variant={variant}
      className={cn(badgeVariants({ variant, shape }), className)}
      {...props}
    />
  );
}

/** The 6px gold dot: changed from default, and nothing else. */
function ChangedDot({ className }: { className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "inline-block size-1.5 shrink-0 rounded-full bg-gold",
        className,
      )}
    />
  );
}

export { Badge, badgeVariants, ChangedDot };
