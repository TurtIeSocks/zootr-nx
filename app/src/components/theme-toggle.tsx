import { Moon, Sun, SunMoon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { THEMES } from "@/lib/theme";
import { useUi } from "@/stores/ui";

const ICONS = { system: SunMoon, light: Sun, dark: Moon };

export function ThemeToggle() {
  const [theme, setTheme] = useUi.usePath("theme");
  const Icon = ICONS[theme];
  const next = () =>
    setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length] ?? theme);
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      onClick={next}
      aria-label={`Theme: ${theme}`}
      title={`Theme: ${theme}`}
    >
      <Icon />
    </Button>
  );
}
