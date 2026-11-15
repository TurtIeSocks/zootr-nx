import { getCurrentWindow } from "@tauri-apps/api/window";
import { type ReactNode, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/app";
import { ErrorBoundary } from "@/components/error-boundary";
import { DevWindow } from "@/dev/dev-window";
import { TrackerWindow } from "@/items/tracker-window";
import { applyTheme, followSystem } from "@/lib/theme";
import { init, useUi } from "@/stores/ui";
import "@/styles/globals.css";

/** The pop outs, by the labels `tauri.conf.json` gives them. */
const WINDOWS: Record<string, ReactNode> = {
  tracker: <TrackerWindow />,
  dev: <DevWindow />,
};

const root = document.getElementById("root");
// The first render waits for the saved view, so no tab shows its defaults
// first. A view that fails to load starts from the defaults.
init()
  .catch((e) => console.error("Couldn't load the view", e))
  .finally(() => {
    followSystem(() => useUi.getState().theme);
    useUi.subscribePath("theme", applyTheme);
    if (root) {
      createRoot(root).render(
        <StrictMode>
          <ErrorBoundary label="This window">
            {WINDOWS[getCurrentWindow().label] ?? <App />}
          </ErrorBoundary>
        </StrictMode>,
      );
    }
  });
