import { useEffect, useState } from "react";
import { ChecksView } from "@/checks/checks-view";
import { ErrorBoundary } from "@/components/error-boundary";
import { LoadGate } from "@/components/load-gate";
import { ItemsView } from "@/items/items-view";
import { cn } from "@/lib/utils";
import { LiveView } from "@/live/live-view";
import { PlayView } from "@/play/play-view";
import { SpoilerSheet } from "@/play/spoiler/spoiler-sheet";
import { SpoilerView } from "@/play/spoiler/spoiler-view";
import { SaveView } from "@/save/save-view";
import { SettingsView } from "@/settings/settings-view";
import { crumbs, type Tab } from "@/shell/nav";
import { Topbar } from "@/shell/page-header";
import { Sidebar } from "@/shell/sidebar";
import { Welcome } from "@/shell/welcome";
import { init as initChecks } from "@/stores/checks";
import { init as initConfig, useConfig } from "@/stores/config";
import { init as initItems } from "@/stores/items";
import { init as initLive } from "@/stores/live";
import { init as initSeeds, useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";
import { init as initUpdate } from "@/stores/update";

/** The view for each tab. */
const VIEWS: Record<Tab | "Settings", () => React.JSX.Element> = {
  Play: PlayView,
  Save: SaveView,
  Checks: ChecksView,
  Items: ItemsView,
  Live: LiveView,
  Spoiler: SpoilerView,
  Settings: SettingsView,
};

/** Views that fill the window and scroll their own rows. */
const FILLS: (Tab | "Settings")[] = ["Checks", "Items", "Live", "Spoiler"];

/** Starts every store the main window reads, and the update check. */
function start() {
  void initLive();
  void initSeeds();
  void initChecks();
  void initItems();
  void initConfig();
  void initUpdate();
}

/** Starts the stores and shows the app once the config and history load. */
export function App() {
  const [config] = useConfig.usePath("loaded");
  const [seeds] = useSeeds.usePath("loaded");
  const [error] = useConfig.usePath("error");
  const [failed] = useSeeds.usePath("error");
  useEffect(start, []);
  return (
    <LoadGate loaded={config && seeds} error={error ?? failed}>
      <First />
    </LoadGate>
  );
}

/** The welcome screen on a first launch (no game dump, no target), until
 * it's done or skipped, then the app. Decided once at load, so saving the
 * dump on the welcome screen doesn't end it. */
function First() {
  const [first] = useState(() => {
    const { config } = useConfig.getState();
    return !config.game && config.targets.length === 0;
  });
  const [done] = useUi.usePath("welcome.done");
  return first && !done ? <Welcome /> : <Shell />;
}

/** The sidebar, the top bar and the tab's view, with History's spoiler
 * sheet over them. */
function Shell() {
  const [tab] = useUi.usePath("tab");
  const [section] = useUi.usePath("section");
  const View = VIEWS[tab];
  const where = crumbs(tab, section).at(-1) ?? tab;
  return (
    <div className="grid h-full grid-cols-[var(--sidebar-width)_minmax(0,1fr)] bg-canvas">
      <Sidebar />
      <main className="flex min-h-0 min-w-0 flex-col">
        <Topbar />
        <div
          className={cn(
            "min-h-0 flex-1 px-8 pt-2",
            FILLS.includes(tab) ? "flex flex-col pb-6" : "overflow-auto pb-12",
          )}
        >
          {/* A view that breaks leaves the sidebar working, and clears when
              another view opens. */}
          <ErrorBoundary label={where} resetKey={`${tab}/${section}`}>
            <View />
          </ErrorBoundary>
        </div>
      </main>
      <SpoilerSheet />
    </div>
  );
}
