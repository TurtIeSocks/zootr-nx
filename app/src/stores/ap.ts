import { create } from "zustand";
import { type ApState, commands, events } from "@/bindings";
import { once } from "@/stores/once";

/** The Archipelago client's status and log, kept current by its events. */
export const useAp = create<ApState>()(() => ({ status: "offline", log: [] }));

/** Listens for the client's notes, then takes what it has said so far. */
const started = once(async () => {
  const unlisten = await events.apEvent.listen(({ payload }) =>
    useAp.setState((s) => ({
      status: payload.status,
      log: payload.line ? [...s.log, payload.line] : s.log,
    })),
  );
  useAp.setState(await commands.apState());
  return unlisten;
});

export const { init, stop } = started;
