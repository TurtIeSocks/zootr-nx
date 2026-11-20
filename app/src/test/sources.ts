/** The app's own source files, path to text, for tests that read code:
 * every `.tsx` and `.ts` under `src/` but tests. */
export const SOURCES = Object.fromEntries(
  Object.entries(
    import.meta.glob<string>(["/src/**/*.{ts,tsx}", "!/src/**/*.test.*"], {
      query: "?raw",
      import: "default",
      eager: true,
    }),
  ),
);
