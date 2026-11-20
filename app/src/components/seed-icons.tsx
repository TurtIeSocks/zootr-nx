import {
  AnchorIcon,
  AxeIcon,
  BirdIcon,
  BombIcon,
  CheckIcon,
  CircleHelpIcon,
  CloverIcon,
  CompassIcon,
  CopyIcon,
  CrownIcon,
  DropletIcon,
  EggIcon,
  FeatherIcon,
  FishIcon,
  FlameIcon,
  GemIcon,
  GhostIcon,
  HammerIcon,
  HeartIcon,
  HourglassIcon,
  KeyIcon,
  LeafIcon,
  type LucideIcon,
  MapIcon,
  MoonIcon,
  MountainIcon,
  MusicIcon,
  ShellIcon,
  ShieldIcon,
  SkullIcon,
  SnowflakeIcon,
  StarIcon,
  SunIcon,
  SwordIcon,
  WandIcon,
  ZapIcon,
} from "lucide-react";
import type { ComponentProps } from "react";
import type { Summary } from "@/bindings";
import { TipButton } from "@/components/tip-button";
import type { Button } from "@/components/ui/button";
import { useCopied } from "@/lib/use-copied";
import { cn } from "@/lib/utils";

/** One medallion tint per family of names. */
const HUES = {
  forest: "bg-medal-forest-soft text-medal-forest",
  fire: "bg-medal-fire-soft text-medal-fire",
  water: "bg-medal-water-soft text-medal-water",
  shadow: "bg-medal-shadow-soft text-medal-shadow",
  spirit: "bg-medal-spirit-soft text-medal-spirit",
  light: "bg-medal-light-soft text-medal-light",
};

/** Every name `icons.rs` can give, with its icon and tint. */
const ICONS: Record<string, [LucideIcon, keyof typeof HUES]> = {
  Leaf: [LeafIcon, "forest"],
  Clover: [CloverIcon, "forest"],
  Feather: [FeatherIcon, "forest"],
  Bird: [BirdIcon, "forest"],
  Gem: [GemIcon, "forest"],
  Flame: [FlameIcon, "fire"],
  Heart: [HeartIcon, "fire"],
  Axe: [AxeIcon, "fire"],
  Bomb: [BombIcon, "fire"],
  Droplet: [DropletIcon, "water"],
  Fish: [FishIcon, "water"],
  Anchor: [AnchorIcon, "water"],
  Snowflake: [SnowflakeIcon, "water"],
  Shell: [ShellIcon, "water"],
  Moon: [MoonIcon, "shadow"],
  Ghost: [GhostIcon, "shadow"],
  Skull: [SkullIcon, "shadow"],
  Music: [MusicIcon, "shadow"],
  Wand: [WandIcon, "shadow"],
  Hourglass: [HourglassIcon, "spirit"],
  Egg: [EggIcon, "spirit"],
  Mountain: [MountainIcon, "spirit"],
  Hammer: [HammerIcon, "spirit"],
  Compass: [CompassIcon, "spirit"],
  Map: [MapIcon, "spirit"],
  Sun: [SunIcon, "light"],
  Star: [StarIcon, "light"],
  Crown: [CrownIcon, "light"],
  Key: [KeyIcon, "light"],
  Zap: [ZapIcon, "light"],
  Sword: [SwordIcon, "light"],
  Shield: [ShieldIcon, "light"],
};

const SIZES = {
  sm: "size-5 rounded-xs [&>svg]:size-3",
  md: "size-7 rounded-sm [&>svg]:size-4",
  lg: "size-10 rounded-md [&>svg]:size-5.5",
};

/** Props for {@link SeedIcons}. */
interface SeedIconsProps {
  icons: string[];
  size?: keyof typeof SIZES;
}

/** A seed's icons, standing in for its number. */
export function SeedIcons({ icons, size = "md" }: SeedIconsProps) {
  return (
    <span
      role="img"
      aria-label={icons.join(", ")}
      className={cn(
        "inline-flex shrink-0",
        size === "lg" ? "gap-1.5" : "gap-1",
      )}
    >
      {icons.map((name, i) => {
        const [Icon, hue] = ICONS[name] ?? [CircleHelpIcon];
        return (
          <span
            // Icons can repeat, so only the position is unique.
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed-length row
            key={i}
            className={cn(
              "grid place-items-center [&>svg]:stroke-2",
              SIZES[size],
              hue ? HUES[hue] : "bg-surface-sunken text-ink-muted",
            )}
          >
            <Icon />
          </span>
        );
      })}
    </span>
  );
}

/** Props for {@link CopySeed}. */
interface CopySeedProps
  extends Pick<ComponentProps<typeof Button>, "variant" | "size"> {
  entry: Pick<Summary, "seed" | "share">;
}

/** Copies the share code: the seed with its settings, for a friend to build
 * the same mod. */
export function CopySeed({ entry, ...props }: CopySeedProps) {
  const { copied, flash } = useCopied();
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(entry.share);
      flash("share");
    } catch {
      // No clipboard to write to.
    }
  };
  return (
    <TipButton
      {...props}
      label="Copy share code"
      tip={
        copied
          ? "Copied"
          : `Copy seed ${entry.seed} with its settings, for New seed`
      }
      onClick={copy}
    >
      {copied ? <CheckIcon /> : <CopyIcon />}
    </TipButton>
  );
}
