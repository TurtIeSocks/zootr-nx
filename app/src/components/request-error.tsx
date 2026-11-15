import { InlineError } from "@/components/callout";
import { useRequest } from "@/lib/request";

/** Props for {@link RequestError}. */
export interface RequestErrorProps {
  className?: string;
}

/** Why the request around it failed, if it did. */
export function RequestError({ className }: RequestErrorProps) {
  return <InlineError error={useRequest().error} className={className} />;
}
