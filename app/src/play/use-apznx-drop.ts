import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useRef, useState } from "react";

/** The first `.apznx` among dropped paths. */
const apznx = (paths: string[]) =>
  paths.find((p) => p.toLowerCase().endsWith(".apznx"));

/** While mounted, a `.apznx` dropped anywhere on the window goes to
 * `open`. Says whether one is being dragged over it now. */
export function useApznxDrop(open: (path: string) => void) {
  const [over, setOver] = useState(false);
  // The latest `open`, so the window listens once rather than per change.
  const latest = useRef(open);
  useEffect(() => {
    latest.current = open;
  });
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let gone = false;
    void getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        if (payload.type === "enter") setOver(!!apznx(payload.paths));
        if (payload.type === "leave") setOver(false);
        if (payload.type === "drop") {
          setOver(false);
          const path = apznx(payload.paths);
          if (path) latest.current(path);
        }
      })
      .then(
        (stop) => {
          if (gone) stop();
          else unlisten = stop;
        },
        (e: unknown) => console.debug("Couldn't listen for drops", e),
      );
    return () => {
      gone = true;
      unlisten?.();
    };
  }, []);
  return over;
}
