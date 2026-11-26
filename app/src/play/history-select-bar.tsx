import { Star } from "lucide-react";
import type { Summary } from "@/bindings";
import { Button } from "@/components/ui/button";
import { ConfirmText } from "@/play/list-row";
import { deleteSeeds, starSeeds, useSeeds } from "@/stores/seeds";

/** Props for {@link SelectBar}. */
interface SelectBarProps {
  /** The picked seeds the filter still shows. */
  picked: Summary[];
  /** Every seed the filter shows, drawn or not. */
  shown: number;
  onSelectAll: () => void;
  onDone: () => void;
}

/** What History's select mode does with the picked seeds. */
export function SelectBar({
  picked,
  shown,
  onSelectAll,
  onDone,
}: SelectBarProps) {
  const [busy] = useSeeds.usePath("busy");
  const [installedId] = useSeeds.usePath("history.installed.id");
  const ids = picked.map((e) => e.id);
  const doomed = picked.filter((e) => e.id !== installedId);
  const keeps = doomed.length < picked.length;
  const starred = doomed.filter((e) => e.starred).length;
  const n = picked.length;
  return (
    <div className="flex flex-wrap items-center gap-1 border-line border-b px-4 pb-2">
      <span className="mr-2 text-[13px] text-ink-muted tabular-nums">
        {n} selected
      </span>
      <Button
        variant="ghost"
        size="sm"
        disabled={n === shown}
        onClick={onSelectAll}
      >
        Select all
      </Button>
      <Button
        variant="ghost"
        size="sm"
        disabled={busy || picked.every((e) => e.starred)}
        onClick={() => starSeeds(ids, true)}
      >
        <Star />
        Star
      </Button>
      <Button
        variant="ghost"
        size="sm"
        disabled={busy || !picked.some((e) => e.starred)}
        onClick={() => starSeeds(ids, false)}
      >
        Unstar
      </Button>
      <ConfirmText
        label={`Delete ${doomed.length}`}
        tip={
          doomed.length === 0 && keeps
            ? "The installed seed can't be deleted"
            : "Delete the selected seeds, starred ones too"
        }
        confirm={`Delete ${doomed.length}${starred ? `, ${starred} starred` : ""}?${keeps ? " Installed stays" : ""}`}
        disabled={busy || doomed.length === 0}
        onConfirm={() => deleteSeeds(doomed.map((e) => e.id))}
      />
      <Button
        variant="secondary"
        size="sm"
        className="ml-auto"
        onClick={onDone}
      >
        Done
      </Button>
    </div>
  );
}
