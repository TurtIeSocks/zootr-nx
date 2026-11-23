import { commands } from "@/bindings";
import { message } from "@/lib/api";

/** The command that takes the lines, which isn't timed itself. */
const WRITE = "appLogWrite";

/** A command's name as the backend spells it. */
const snake = (name: string) =>
  name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);

/** How a command ended: "ok", or its error. */
function outcome(result: unknown): string {
  const r = result as { status?: string; error?: unknown } | null;
  return r?.status === "error" ? message(r.error) : "ok";
}

let wrapped = false;

/** Wraps each command once, so that while `on()` holds, each call sends
 * the app log a debug line with its time. A command that ended the same
 * way within a minute is skipped there, so polls don't flood it. */
export function timeCommands(on: () => boolean) {
  if (wrapped) return;
  wrapped = true;
  const all = commands as unknown as Record<
    string,
    (...args: unknown[]) => Promise<unknown>
  >;
  for (const [name, run] of Object.entries(all)) {
    if (name === WRITE || typeof run !== "function") continue;
    const command = snake(name);
    all[name] = async (...args) => {
      if (!on()) return run(...args);
      const began = performance.now();
      const log = (ended: string) => {
        const secs = ((performance.now() - began) / 1000).toFixed(2);
        const failed = ended === "ok" ? "" : ", failed";
        void commands
          .appLogWrite(
            "debug",
            `→ ${command} ${secs} s${failed}`,
            `${command} ${ended}`,
          )
          .catch(() => {});
      };
      try {
        const result = await run(...args);
        log(outcome(result));
        return result;
      } catch (e) {
        log(message(e));
        throw e;
      }
    };
  }
}
