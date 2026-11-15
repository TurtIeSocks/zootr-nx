import type * as React from "react";
import { fieldClass } from "@/components/styles";
import { cn } from "@/lib/utils";

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      rows={3}
      className={cn(
        fieldClass,
        "min-h-18 resize-y px-3 py-2 leading-5",
        className,
      )}
      {...props}
    />
  );
}

export { Textarea };
