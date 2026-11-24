import { getVersion } from "@tauri-apps/api/app";
import { create } from "zustand";
import { dotPath } from "zustand-dot";
import { init as initConfig, useConfig } from "@/stores/config";
import { once } from "@/stores/once";
import { useUi } from "@/stores/ui";
import {
  fetchLatest,
  newer,
  RELEASES_REPO,
  type Release,
} from "@/update/release";

/** The update notice's state. Not saved: each launch checks again. */
interface Update {
  /** A release newer than this build, or null. */
  latest: Release | null;
  /** This build's version, once read. */
  current: string | null;
}

export const useUpdate = create<Update>()(
  dotPath((): Update => ({ latest: null, current: null })),
);

/** How often to check again after launch. */
const EVERY = 6 * 60 * 60 * 1000;

/** The repo to check: `VITE_UPDATE_REPO` when set, else the real one,
 * except in dev, which checks nothing. */
export function repo(): string | null {
  const env = import.meta.env.VITE_UPDATE_REPO;
  if (env) return env;
  return import.meta.env.DEV ? null : RELEASES_REPO;
}

/** The repo a check you ask for goes to: `VITE_UPDATE_REPO` when set,
 * else the real one, in dev too. */
export const askedRepo = () =>
  import.meta.env.VITE_UPDATE_REPO || RELEASES_REPO;

/** What a check you asked for found. */
export type Checked = "newer" | "current" | "failed";

/** `latest` when it's newer than this build, else null. */
function newerOnly(latest: Release | null): Release | null {
  const { current } = useUpdate.getState();
  return latest && current && newer(current, latest.version) ? latest : null;
}

/** Asks GitHub for the latest release now, even with checks off. A newer
 * one shows in the sidebar, even one skipped before. */
export async function checkNow(): Promise<Checked> {
  const found = await fetchLatest(askedRepo());
  if (!found) return "failed";
  const latest = newerOnly(found);
  if (!latest) return "current";
  useUi.setPath("update.skipped", null);
  useUpdate.setPath("latest", latest);
  return "newer";
}

/** Counts the checks begun, so only the newest one's answer lands. */
let checks = 0;

/** Asks GitHub for the latest release, unless checks are off. */
async function check() {
  const asked = ++checks;
  const from = repo();
  if (!from || useConfig.getState().config.no_update_check) {
    useUpdate.setPath("latest", null);
    return;
  }
  const latest = await fetchLatest(from);
  if (asked === checks) useUpdate.setPath("latest", newerOnly(latest));
}

/** Reads this build's version, which the sidebar always shows, then checks
 * once the config is read, every 6 hours and whenever the switch flips. */
const started = once(async () => {
  await getVersion().then(
    (v) => useUpdate.setPath("current", v),
    (e: unknown) => console.debug("Couldn't read the version", e),
  );
  await initConfig();
  await check();
  const timer = setInterval(() => void check(), EVERY);
  const off = useConfig.subscribePath(
    "config.no_update_check",
    () => void check(),
  );
  return () => {
    clearInterval(timer);
    off();
  };
});

export const { init, stop } = started;

/** Hides the notice until a release newer than `version`. */
export const skip = (version: string) =>
  useUi.setPath("update.skipped", version);
