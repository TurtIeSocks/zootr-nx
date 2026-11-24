import type { ComponentProps } from "react";
import { useLogKeys } from "@/components/log-view/keys";
import type { CopyButton } from "@/components/log-view/log-tools";
import type { Selection } from "@/components/log-view/use-selection";
import { useCopied } from "@/lib/use-copied";

/** The props of a log's {@link CopyButton}, which copies the selected lines
 * or every one shown, with the keys of {@link useLogKeys} wired up. */
export function useLogCopy<L extends { id: number }>(
  shown: L[],
  selection: Selection,
  textOf: (lines: L[]) => string,
): ComponentProps<typeof CopyButton> {
  const { copied, copy } = useCopied();
  const selecting = selection.ids.size > 0;
  const lines = selecting
    ? shown.filter((l) => selection.ids.has(l.id))
    : shown;
  const onCopy = () => copy("lines", textOf(lines));
  useLogKeys(shown, selection, onCopy);
  return {
    count: lines.length,
    selecting,
    copied: copied !== undefined,
    onCopy,
  };
}
