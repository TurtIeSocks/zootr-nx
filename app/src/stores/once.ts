/** `start` run once, however often `init` is called, until `stop`. `start`
 * resolves to what undoes it. */
export function once(start: () => Promise<() => void>) {
  let running: Promise<() => void> | undefined;
  return {
    init: async () => {
      running ??= start();
      await running;
    },
    stop: () => {
      running?.then((undo) => undo());
      running = undefined;
    },
  };
}
