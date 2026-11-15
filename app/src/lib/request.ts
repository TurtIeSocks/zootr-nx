import {
  createContext,
  use,
  useCallback,
  useMemo,
  useRef,
  useState,
} from "react";
import { message } from "@/lib/api";

/** What the last request said back, or why it failed. */
export interface Said {
  /** False when it failed. */
  ok: boolean;
  text: string;
}

/** A request: whether it's running, what it said, and how to run or clear
 * it. */
export interface RequestController {
  /** A request is running. */
  busy: boolean;
  /** What the last one said, until the next one starts or `clear`. */
  said: Said | undefined;
  /** Why the last one failed, when it did. */
  error: string | undefined;
  /** Runs `req` unless one is already running. A string it returns becomes
   * what it said; anything else says nothing. A throw becomes the error. */
  run: (req: () => Promise<unknown>) => Promise<void>;
  /** Drops what the last one said. */
  clear: () => void;
}

/** Makes a request of the caller's own: one at a time, keeping what it said
 * or why it failed. A component may make more than one. */
export function useRequestState(): RequestController {
  const [busy, setBusy] = useState(false);
  const [said, setSaid] = useState<Said>();
  const running = useRef(false);

  const run = useCallback(async (req: () => Promise<unknown>) => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setSaid(undefined);
    try {
      const result = await req();
      if (typeof result === "string") setSaid({ ok: true, text: result });
    } catch (e) {
      setSaid({ ok: false, text: message(e) });
    } finally {
      running.current = false;
      setBusy(false);
    }
  }, []);
  const clear = useCallback(() => setSaid(undefined), []);

  return useMemo(
    () => ({
      busy,
      said,
      error: said?.ok ? undefined : said?.text,
      run,
      clear,
    }),
    [busy, said, run, clear],
  );
}

/** The request around a subtree. Kept apart from its provider, so the
 * provider's file exports only components and Fast Refresh keeps its
 * state. */
export const RequestContext = createContext<RequestController | null>(null);

/** The request of the `RequestProvider` around the caller. Throws outside
 * one. */
export function useRequest() {
  const request = use(RequestContext);
  if (!request)
    throw new Error("useRequest has to be used within <RequestProvider>");
  return request;
}
