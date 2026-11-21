import { type ComponentType, useEffect } from "react";
import { Activity } from "@/dev/activity";
import { ActorsTab, LiveReads } from "@/dev/actors-tab";
import { GiveTab } from "@/dev/give-tab";
import { Header } from "@/dev/header";
import { PANEL, tabId, usePaletteKey } from "@/dev/nav";
import { NotRunning } from "@/dev/not-running";
import { Palette } from "@/dev/palette";
import { Tabs } from "@/dev/tabs";
import { TestsTab } from "@/dev/tests-tab";
import { ValuesTab } from "@/dev/values-tab";
import { WarpTab } from "@/dev/warp-tab";
import { init, useDev } from "@/stores/dev";
import { type DevTab, useUi } from "@/stores/ui";

/** Each tab's body. */
const BODIES: Record<DevTab, ComponentType> = {
  warp: WarpTab,
  actors: ActorsTab,
  give: GiveTab,
  values: ValuesTab,
  tests: TestsTab,
};

/** The dev panel's window: a fixed header, five tabs and the activity
 * line, with Find anything over it on ⌘K. It floats over the game until
 * unpinned. */
export function DevWindow() {
  useEffect(() => {
    void init();
  }, []);
  usePaletteKey();

  return (
    <div className="relative flex h-full flex-col bg-canvas text-ink">
      <LiveReads />
      <Header />
      <Tabs />
      <DevError />
      <Body />
      <Activity />
      <Palette />
    </div>
  );
}

/** The open tab's body, or how to start the game while it can't be read
 * and no tab was picked since. */
function Body() {
  const [tab] = useUi.usePath("dev.tab");
  const [{ state, error }] = useDev.usePath("game");
  const [browsing] = useDev.usePath("browsing");
  const Tab = BODIES[tab];

  return (
    <div
      role="tabpanel"
      id={PANEL}
      aria-labelledby={tabId(tab)}
      className="flex min-h-0 flex-1 flex-col overflow-auto p-3"
    >
      {!state && error && !browsing ? <NotRunning /> : <Tab />}
    </div>
  );
}

/** Why the dev options didn't load. */
function DevError() {
  const [error] = useDev.usePath("error");

  if (!error) return null;
  return (
    <p className="bg-danger-soft px-3 py-1.5 text-danger-ink text-xs">
      {error}
    </p>
  );
}
