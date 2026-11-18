import { Globe, History, Sparkles } from "lucide-react";
import { FolderTabs } from "@/components/folder-tabs";
import { ApClient } from "@/play/ap-client";
import { HistoryList } from "@/play/history-list";
import { NewSeed, OpenApButton } from "@/play/new-seed";
import { useAp } from "@/stores/ap";
import { useSeeds } from "@/stores/seeds";
import { type PlayTab, useUi } from "@/stores/ui";

/** The folder the Play page opens on: the one last open, else the one for
 * the installed seed's kind. */
function useOpenTab(): PlayTab {
  const [saved] = useUi.usePath("play.tab");
  const ap = useSeeds((s) => !!s.history.installed?.ap_slot);
  return saved ?? (ap ? "archipelago" : "standard");
}

/** Standard, Archipelago and History, under the installed seed. */
export function PlayTabs() {
  const tab = useOpenTab();
  const connected = useAp((s) => s.status === "connected");
  const installed = useSeeds((s) => s.history.installed);
  const n = useSeeds((s) => s.history.entries.length);
  return (
    <FolderTabs
      label="Make or pick a seed"
      value={tab}
      onValueChange={(id) => useUi.setPath("play.tab", id)}
      tabs={[
        {
          id: "standard",
          label: "Standard",
          icon: Sparkles,
          panel: <NewSeed />,
        },
        {
          id: "archipelago",
          label: "Archipelago",
          icon: Globe,
          meta: connected && (
            <span
              role="img"
              aria-label="connected"
              className="size-[7px] rounded-full bg-primary"
            />
          ),
          panel: (
            <div className="flex flex-col gap-3 p-4">
              <OpenApButton />
              {installed?.ap_slot && <ApClient entry={installed} />}
            </div>
          ),
        },
        {
          id: "history",
          label: "History",
          icon: History,
          meta: n > 0 && (
            <span className="font-[450] text-ink-faint text-xs tabular-nums">
              {n}
            </span>
          ),
          panel: <HistoryList />,
        },
      ]}
    />
  );
}
