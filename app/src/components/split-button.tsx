import { ChevronDownIcon, type LucideIcon } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { Button } from "@/components/ui/button";
import {
  ButtonGroup,
  ButtonGroupSeparator,
} from "@/components/ui/button-group";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { WhyButton } from "@/components/why-button";

/** One thing a {@link SplitButton} does. */
export interface SplitOption {
  icon: LucideIcon;
  label: string;
  /** What it does, under its label in the menu. */
  line: string;
  /** Why it can't act now, or false when it can. */
  why: ReactNode | false;
  act: () => void;
  /** Runs on the lead's click while it can't act. */
  onWhy?: () => void;
}

/** Props for {@link SplitButton}. */
interface SplitButtonProps {
  options: SplitOption[];
  /** The option on the button itself. */
  lead: SplitOption;
  /** What the chevron opens, for a screen reader. */
  more: string;
  variant?: ComponentProps<typeof Button>["variant"];
  /** Shown in place of the lead's icon: a spinner while it runs. */
  icon?: ReactNode;
}

/** A button doing `lead`, and a chevron beside it opening every option. */
export function SplitButton({
  options,
  lead,
  more,
  variant,
  icon,
}: SplitButtonProps) {
  return (
    <ButtonGroup>
      <WhyButton
        variant={variant}
        why={lead.why}
        onWhy={lead.onWhy}
        onClick={lead.act}
      >
        {icon ?? <lead.icon />}
        {lead.label}
      </WhyButton>
      <ButtonGroupSeparator />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant={variant} className="w-8 px-0" aria-label={more}>
            <ChevronDownIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-[300px]">
          {options.map(({ icon: Icon, label, line, why, act }) => (
            <DropdownMenuItem
              key={label}
              className="h-auto items-start py-2"
              disabled={why !== false}
              onSelect={act}
            >
              <Icon className="mt-0.5" />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="text-ink">{label}</span>
                <span className="text-ink-muted text-xs">{why || line}</span>
              </span>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </ButtonGroup>
  );
}
