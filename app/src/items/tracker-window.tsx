import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { useEffect, useRef } from "react";
import { ItemsGrid } from "@/items/items-grid";
import { init, useItems } from "@/stores/items";

/** The pop out: the grid alone on its background, for OBS to capture. The
 * window follows the grid's size. */
export function TrackerWindow() {
  const [ready] = useItems.usePath("loaded");
  const [background] = useItems.usePath("options.background");
  const grid = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void init();
  }, []);

  useEffect(() => {
    if (ready) document.body.style.background = background;
  }, [ready, background]);

  useEffect(() => {
    const el = grid.current;
    if (!ready || !el) return;
    const fit = new ResizeObserver(() => {
      const { width, height } = el.getBoundingClientRect();
      getCurrentWindow().setSize(
        new LogicalSize(Math.ceil(width), Math.ceil(height)),
      );
    });
    fit.observe(el);
    return () => fit.disconnect();
  }, [ready]);

  if (!ready) return null;
  return (
    <div ref={grid} className="inline-block">
      <ItemsGrid />
    </div>
  );
}
