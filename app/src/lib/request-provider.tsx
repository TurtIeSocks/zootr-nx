import type { PropsWithChildren } from "react";
import { RequestContext, useRequestState } from "@/lib/request";

/** Gives its subtree one request to share, read with `useRequest`. */
export function RequestProvider({ children }: PropsWithChildren) {
  return <RequestContext value={useRequestState()}>{children}</RequestContext>;
}
