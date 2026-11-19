import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useEffect, useState } from "react";

/** The first `.apznx` among dropped paths. */
const apznx = (paths: string[]) =>
  paths.find((p) => p.toLowerCase().endsWith(".apznx"));

/** While mounted, a `.apznx` dropped anywhere on the window goes to
 * `open`. Says whether one is being dragged over it now. */
export function useApznxDrop(open: (path: string) => void) {
  const [over, setOver] = useState(false);
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
          if (path) open(path);
        }
      })
      .then((stop) => {
        if (gone) stop();
        else unlisten = stop;
      });
    return () => {
      gone = true;
      unlisten?.();
    };
  }, [open]);
  return over;
}
