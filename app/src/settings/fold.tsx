import { Disclosure, type DisclosureProps } from "@/components/disclosure";
import { setSectionOpen, useUi } from "@/stores/ui";

/** Props for {@link Fold}. */
export interface FoldProps
  extends Omit<DisclosureProps, "open" | "onOpenChange" | "lazy"> {
  /** Keys whether it's open, across restarts. */
  id: string;
  title: string;
}

/** A settings page's section. Each starts open, and stays as it was
 * left. */
export function Fold({ id, ...props }: FoldProps) {
  const open = useUi((s) => !s.settings.closed.includes(id));
  return (
    <Disclosure
      open={open}
      onOpenChange={(open) => setSectionOpen(id, open)}
      {...props}
    />
  );
}
