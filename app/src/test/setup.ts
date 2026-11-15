import { afterEach } from "vitest";

// Imported when first needed, so the stores take a test file's mock of
// `@/bindings` rather than the real one.
afterEach(async () => {
  const { resetStores } = await import("@/stores/reset");
  resetStores();
});
