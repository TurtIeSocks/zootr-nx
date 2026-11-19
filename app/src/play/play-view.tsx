import { useEffect } from "react";
import { InstalledSeed } from "@/play/installed-seed";
import { LabelDialog } from "@/play/label-dialog";
import { PlayTabs } from "@/play/play-tabs";
import { Page } from "@/shell/page-header";
import { init } from "@/stores/ap";
import { useSeeds } from "@/stores/seeds";

/** What's installed, then a folder each for a standard seed, an
 * Archipelago slot and every seed before. */
export function PlayView() {
  // Only an AP slot connects to a room, so only it listens to the client.
  const ap = useSeeds((s) => !!s.history.installed?.ap_slot);
  useEffect(() => {
    if (ap) void init();
  }, [ap]);
  return (
    <Page>
      <InstalledSeed />
      <PlayTabs />
      <LabelDialog />
    </Page>
  );
}
