import { useRequest } from "@/lib/request";
import { saveConfig } from "@/stores/config";

/** Saves a config change through the request around the caller. */
export function useSaveConfig() {
  const { run } = useRequest();
  return (change: Parameters<typeof saveConfig>[0]) =>
    run(() => saveConfig(change));
}
