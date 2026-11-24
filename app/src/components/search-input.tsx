import { Search } from "lucide-react";
import type * as React from "react";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

/** Props for {@link SearchInput}: an input's, with `className` on the box
 * around it. */
type SearchInputProps = React.ComponentProps<"input">;

/** A small input with a search icon in it. */
export function SearchInput({ className, ...props }: SearchInputProps) {
  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-ink-faint" />
      <Input className="h-7 pl-8" {...props} />
    </div>
  );
}
