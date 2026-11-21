/** The ids of rows the search reveals that have no control of their own. */
export const rowId = {
  check: (hash: string) => `check-${hash}`,
  seed: (id: number) => `seed-${id}`,
};

/** How long reveal waits for the page to draw its element. */
const TRIES = 30;
const EVERY = 16;
/** The class that flashes a revealed row, as long as `--duration-flash`. */
const FLASH = "animate-flash";

/** Waits for the element `id` to draw, opens any `<details>` around it,
 * scrolls it to the middle, focuses it, and flashes its row (its nearest
 * `[data-reveal]`). Gives up quietly when it never draws. */
export function reveal(id: string, tries = TRIES) {
  const el = document.getElementById(id);
  if (!el || el.closest("[inert]")) {
    if (tries > 0) setTimeout(() => reveal(id, tries - 1), EVERY);
    return;
  }
  for (
    let d = el.closest("details");
    d;
    d = d.parentElement?.closest("details") ?? null
  )
    d.open = true;
  const row = el.closest<HTMLElement>("[data-reveal]") ?? el;
  row.scrollIntoView?.({ block: "center", behavior: "smooth" });
  el.focus({ preventScroll: true });
  row.classList.remove(FLASH);
  // Read layout, so the animation starts again on a second reveal.
  void row.offsetWidth;
  row.classList.add(FLASH);
  row.addEventListener("animationend", () => row.classList.remove(FLASH), {
    once: true,
  });
}
