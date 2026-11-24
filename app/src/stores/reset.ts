import type { StoreApi } from "zustand";
import * as ap from "@/stores/ap";
import * as appLog from "@/stores/app-log";
import * as checks from "@/stores/checks";
import * as config from "@/stores/config";
import * as dev from "@/stores/dev";
import * as items from "@/stores/items";
import * as live from "@/stores/live";
import * as seeds from "@/stores/seeds";
import { useShell } from "@/stores/shell";
import { useUi } from "@/stores/ui";
import * as update from "@/stores/update";

const reset =
  <T>(
    store: Pick<StoreApi<T>, "getInitialState" | "setState">,
    stop?: () => void,
  ) =>
  () => {
    stop?.();
    store.setState(store.getInitialState(), true);
  };

/** Every store, stopped and back at its initial state, so each test starts
 * afresh. */
const RESETS = [
  reset(useUi),
  reset(live.useLive, live.stop),
  reset(items.useItems, items.stop),
  reset(config.useConfig, config.stop),
  reset(seeds.useSeeds, seeds.stop),
  reset(checks.useChecks, checks.stop),
  reset(useShell),
  reset(dev.useDev, dev.stop),
  reset(ap.useAp, ap.stop),
  reset(appLog.useAppLog, appLog.stop),
  reset(update.useUpdate, update.stop),
];

export function resetStores() {
  for (const r of RESETS) r();
}
