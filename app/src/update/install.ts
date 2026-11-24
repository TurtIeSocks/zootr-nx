import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

/** The updater's update when it can install `version`, else null: a release
 * without updater files, a feed that lags the notice, or a failed check. */
export async function installable(version: string): Promise<Update | null> {
  try {
    const update = await check();
    if (update?.version === version) return update;
    await update?.close().catch(() => {});
    return null;
  } catch (e) {
    console.debug("Couldn't check for an installable update", e);
    return null;
  }
}

/** Bytes downloaded so far, and the total when the server gave one. */
export interface Progress {
  done: number;
  total: number | null;
}

/** Downloads `update`, checks its signature, installs it and restarts the
 * app on it. Throws, without restarting, when any step fails. */
export async function install(
  update: Update,
  onProgress: (p: Progress) => void,
): Promise<void> {
  const p: Progress = { done: 0, total: null };
  await update.downloadAndInstall((e) => {
    if (e.event === "Started") p.total = e.data.contentLength ?? null;
    else if (e.event === "Progress") p.done += e.data.chunkLength;
    else return;
    onProgress({ ...p });
  });
  await relaunch();
}
