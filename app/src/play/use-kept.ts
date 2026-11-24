import { useState } from "react";

/** `value`, or the last one it held while it's unset: what a dialog keeps
 * showing while it closes, so its content doesn't blank during the fade. */
export function useKept<T>(value: T | undefined) {
  const [kept, setKept] = useState(value);
  if (value !== undefined && value !== kept) setKept(value);
  return value ?? kept;
}
