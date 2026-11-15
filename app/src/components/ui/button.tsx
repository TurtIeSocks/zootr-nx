import { cva, type VariantProps } from "class-variance-authority";
import { Slot } from "radix-ui";
import type * as React from "react";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex shrink-0 cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-md border border-transparent font-medium text-[13px] leading-none outline-none transition-[background-color,border-color,color,box-shadow,transform] duration-(--duration-fast) ease-out enabled:active:translate-y-[0.5px] enabled:active:scale-[0.985] disabled:cursor-not-allowed disabled:opacity-45 aria-disabled:cursor-not-allowed aria-disabled:opacity-45 [&_svg:not([class*='size-'])]:size-4 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg]:stroke-[1.75]",
  {
    variants: {
      variant: {
        default:
          "bg-primary text-on-primary shadow-button enabled:hover:bg-primary-hover",
        secondary:
          "border-line-strong bg-surface text-ink shadow-xs enabled:hover:bg-surface-hover",
        ghost:
          "bg-transparent text-ink-muted enabled:hover:bg-surface-hover enabled:hover:text-ink",
        soft: "bg-primary-soft text-primary-ink enabled:hover:bg-primary enabled:hover:text-on-primary",
        destructive: "bg-danger text-on-danger shadow-button",
        "danger-ghost":
          "bg-transparent text-danger-ink enabled:hover:bg-danger-soft",
        selected:
          "border-line-strong bg-primary-soft text-primary-ink shadow-xs",
      },
      size: {
        default: "h-8 px-3",
        sm: "h-7 rounded-sm px-2 text-[12.5px]",
        lg: "h-10 px-4 text-sm",
        icon: "size-8",
        "icon-xs": "size-6 rounded-xs [&_svg:not([class*='size-'])]:size-3.5",
        "icon-sm": "size-7 rounded-sm",
        "icon-lg": "size-10",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean;
  }) {
  const Comp = asChild ? Slot.Root : "button";

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );
}

export { Button, buttonVariants };
