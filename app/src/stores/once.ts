/** `start` run once, however often `init` is called, until `stop`. `start`
 * resolves to what undoes it. One that fails runs again on the next
 * `init`. */
export function once(start: () => Promise<() => void>) {
  let running: Promise<() => void> | undefined;
  return {
    init: async () => {
      if (!running) {
        // A start that fails after a stop and a new start leaves the new
        // one be.
        const started: Promise<() => void> = start().catch((e: unknown) => {
          if (running === started) running = undefined;
          throw e;
        });
        running = started;
      }
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
