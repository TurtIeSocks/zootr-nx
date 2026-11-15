import type * as React from "react";
import { fieldClass } from "@/components/styles";
import { cn } from "@/lib/utils";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(fieldClass, "h-8 px-3", className)}
      {...props}
    />
  );
}

export { Input };
