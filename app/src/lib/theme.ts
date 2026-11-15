export type Theme = "system" | "light" | "dark";

/** In the order the toggle steps through them. */
export const THEMES: Theme[] = ["system", "light", "dark"];

export const isDark = (theme: Theme, systemDark: boolean) =>
  theme === "dark" || (theme === "system" && systemDark);

const system = () => window.matchMedia?.("(prefers-color-scheme: dark)");

export function applyTheme(theme: Theme) {
  const dark = isDark(theme, system()?.matches ?? false);
  document.documentElement.classList.toggle("dark", dark);
}

/** Applies `theme()` now, and again whenever the system's scheme changes. */
export function followSystem(theme: () => Theme) {
  applyTheme(theme());
  system()?.addEventListener("change", () => applyTheme(theme()));
}
