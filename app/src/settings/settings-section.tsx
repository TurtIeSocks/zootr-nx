import type { ReactNode } from "react";
import { Card } from "@/components/card";
import { divided } from "@/components/styles";
import { cn } from "@/lib/utils";

/** Props for {@link SettingsSection}. */
export interface SettingsSectionProps {
  /** The setting rows. */
  children: ReactNode;
}

/** A card of setting rows, divided by inset rules that vanish around the
 * row under the pointer. */
export function SettingsSection({ children }: SettingsSectionProps) {
  return (
    <Card>
      <div className={cn("p-1", divided)}>{children}</div>
    </Card>
  );
}
