import { FolderTabs } from "@/components/folder-tabs";
import { ArchipelagoPanel } from "@/play/archipelago-panel";
import { FavoritesList } from "@/play/favorites-list";
import { HistoryList } from "@/play/history-list";
import { NewSeed } from "@/play/new-seed";
import { PLAY_FOLDERS } from "@/shell/nav";
import { useAp } from "@/stores/ap";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { type PlayTab, useUi } from "@/stores/ui";

/** The folder the Play page opens on: the one last open, else the one for
 * the installed seed's kind. */
function useOpenTab(): PlayTab {
  const [saved] = useUi.usePath("play.tab");
  const ap = useSeeds((s) => !!s.history.installed?.ap_slot);
  return saved ?? (ap ? "archipelago" : "standard");
}

/** Standard, Archipelago, History and Favorites, under the installed
 * seed. */
export function PlayTabs() {
  const tab = useOpenTab();
  const connected = useAp((s) => s.status === "connected");
  const n = useSeeds((s) => s.history.entries.length);
  const favorites = useConfig((s) => s.config.favorites.length);
  return (
    <FolderTabs
      label="Make or pick a seed"
      value={tab}
      onValueChange={(id) => useUi.setPath("play.tab", id)}
      tabs={[
        {
          id: "standard",
          ...PLAY_FOLDERS.standard,
          panel: <NewSeed />,
        },
        {
          id: "archipelago",
          ...PLAY_FOLDERS.archipelago,
          meta: connected && (
            <span
              role="img"
              aria-label="connected"
              className="size-[7px] rounded-full bg-primary"
            />
          ),
          panel: <ArchipelagoPanel />,
        },
        {
          id: "history",
          ...PLAY_FOLDERS.history,
          meta: n > 0 && <Count n={n} />,
          panel: <HistoryList />,
        },
        {
          id: "favorites",
          ...PLAY_FOLDERS.favorites,
          meta: favorites > 0 && <Count n={favorites} />,
          panel: <FavoritesList />,
        },
      ]}
    />
  );
}

/** A folder tab's count. */
function Count({ n }: { n: number }) {
  return (
    <span className="font-[450] text-ink-faint text-xs tabular-nums">{n}</span>
  );
}
