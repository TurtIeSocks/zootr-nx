/** `start` run once, however often `init` is called, until `stop`. `start`
 * resolves to what undoes it. One that fails runs again on the next
 * `init`. */
export function once(start: () => Promise<() => void>) {
  let running: Promise<() => void> | undefined;
  return {
    init: async () => {
      running ??= start().catch((e: unknown) => {
        running = undefined;
        throw e;
      });
      await running;
    },
    stop: () => {
      running?.then(
        (undo) => undo(),
        () => {},
      );
      running = undefined;
    },
  };
}
