import { useState } from "react";

/** A destructive action that asks for a second click. Leaving it, by
 * pointer or focus, disarms: a click doesn't focus a button in WebKit, so
 * blur alone could leave it armed. Spread everything but `armed` onto the
 * button. */
export function useArmed(act: () => void) {
  const [armed, setArmed] = useState(false);
  return {
    armed,
    onClick: () => {
      setArmed(!armed);
      if (armed) act();
    },
    onBlur: () => setArmed(false),
    onPointerLeave: () => setArmed(false),
  };
}
