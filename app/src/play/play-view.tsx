import { HistoryList } from "@/play/history-list";
import { InstalledSeed } from "@/play/installed-seed";
import { LabelDialog } from "@/play/label-dialog";
import { NewSeed } from "@/play/new-seed";
import { Page } from "@/shell/page-header";

/** What's installed, a new seed, and every seed before it. */
export function PlayView() {
  return (
    // History takes the height left over and scrolls its own rows.
    <Page className="h-full">
      <InstalledSeed />
      <NewSeed />
      <HistoryList />
      <LabelDialog />
    </Page>
  );
}
