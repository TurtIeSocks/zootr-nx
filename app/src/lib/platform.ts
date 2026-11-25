import { convertFileSrc, isTauri } from "@tauri-apps/api/core";

/** Whether the app runs on macOS, where the window buttons sit inside the
 * sidebar's title band. Elsewhere the title bar is native, above the app. */
export const MAC = navigator.userAgent.includes("Mac");

/** The modifier key the shortcuts take, as the menus and tips write it. */
export const MOD = MAC ? "⌘" : "Ctrl+";

/** A URL for `path` on one of the app's own schemes (`zootr-icon` and the
 * like). Windows' webview reaches them only as `http://<scheme>.localhost/`,
 * so Tauri builds the base. Outside the app, as in tests, the plain form. */
export function schemeUrl(scheme: string, path: string) {
  const base = isTauri()
    ? convertFileSrc("", scheme)
    : `${scheme}://localhost/`;
  return base + path;
}
