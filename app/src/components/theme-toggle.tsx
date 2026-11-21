import { Moon, Sun, SunMoon } from "lucide-react";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";
import { nextTheme, useUi } from "@/stores/ui";

const ICONS = { system: SunMoon, light: Sun, dark: Moon };

export function ThemeToggle() {
  const [theme] = useUi.usePath("theme");
  const Icon = ICONS[theme];
  return (
    <Tip tip={`Theme: ${theme}`}>
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={nextTheme}
        aria-label={`Theme: ${theme}`}
      >
        <Icon />
      </Button>
    </Tip>
  );
}
