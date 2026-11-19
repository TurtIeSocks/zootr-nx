import { afterEach, vi } from "vitest";

// jsdom has no webview to drop files on. A test that drops one mocks this
// itself.
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: () => Promise.resolve(() => {}),
  }),
}));

// Imported when first needed, so the stores take a test file's mock of
// `@/bindings` rather than the real one.
afterEach(async () => {
  const { resetStores } = await import("@/stores/reset");
  resetStores();
});
