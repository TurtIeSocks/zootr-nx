import { useState } from "react";

/** A destructive action that asks for a second click. Leaving it disarms.
 * Spread everything but `armed` onto the button. */
export function useArmed(act: () => void) {
  const [armed, setArmed] = useState(false);
  return {
    armed,
    onClick: () => (armed ? act() : setArmed(true)),
    onBlur: () => setArmed(false),
  };
}
