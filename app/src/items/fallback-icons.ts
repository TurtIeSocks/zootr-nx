import {
  Anchor,
  Axe,
  Backpack,
  Bean,
  Bird,
  Bomb,
  Bug,
  CircleHelp,
  Compass,
  Crosshair,
  Diamond,
  Drama,
  Droplet,
  Droplets,
  Egg,
  Eye,
  Feather,
  Flame,
  FlaskRound,
  Footprints,
  Gem,
  Glasses,
  Hammer,
  Hand,
  Heart,
  Hexagon,
  Hourglass,
  Key,
  KeyRound,
  type LucideIcon,
  Mail,
  Map as MapIcon,
  Moon,
  Music,
  Music2,
  Nut,
  Package,
  Rat,
  Receipt,
  Redo2,
  RotateCw,
  ScrollText,
  Shield,
  ShieldPlus,
  Shirt,
  ShoppingBag,
  Skull,
  Snowflake,
  Sparkles,
  Sprout,
  Sun,
  Sword,
  Target,
  Ticket,
  TreePine,
  Vibrate,
  Wallet,
  WandSparkles,
  Waves,
  Wind,
  Zap,
} from "lucide-react";
import type { Glyph } from "@/bindings";

/** zootr-nx's own icon for each glyph a cell can carry. The app ships
 * none of the game's art, so these draw until game assets are on. A glyph
 * without an icon fails the typecheck. */
export const FALLBACK_ICONS: Record<Glyph, LucideIcon> = {
  sticks: WandSparkles,
  nuts: Nut,
  bombs: Bomb,
  bombchus: Rat,
  bow: Target,
  slingshot: Crosshair,
  ocarina: Music,
  hookshot: Anchor,
  boomerang: Redo2,
  beans: Bean,
  lens: Glasses,
  hammer: Hammer,
  "fire-arrow": Flame,
  "fire-medallion": Flame,
  "ice-arrow": Snowflake,
  "light-arrow": Sun,
  "light-medallion": Sun,
  "dins-fire": Zap,
  "farores-wind": Wind,
  "nayrus-love": Diamond,
  bottle: FlaskRound,
  egg: Egg,
  cucco: Bird,
  letter: Mail,
  mask: Drama,
  "truth-mask": Eye,
  mushroom: Sprout,
  saw: Axe,
  sword: Sword,
  prescription: ScrollText,
  frog: Bug,
  "eye-drops": Droplets,
  "claim-check": Receipt,
  shield: Shield,
  tunic: Shirt,
  "iron-boots": Footprints,
  "hover-boots": Feather,
  gauntlets: Hand,
  scale: Waves,
  wallet: Wallet,
  "bomb-bag": Backpack,
  quiver: Package,
  "bullet-bag": ShoppingBag,
  "heart-piece": Heart,
  heart: Heart,
  rupee: Hexagon,
  magic: Sparkles,
  spin: RotateCw,
  defense: ShieldPlus,
  "gerudo-card": Ticket,
  "agony-stone": Vibrate,
  skulltula: Skull,
  song: Music2,
  stone: Gem,
  "forest-medallion": TreePine,
  "water-medallion": Droplet,
  "spirit-medallion": Hourglass,
  "shadow-medallion": Moon,
  "small-key": Key,
  "boss-key": KeyRound,
  map: MapIcon,
  compass: Compass,
  unknown: CircleHelp,
};

/** A `#rrggbb` colour's WCAG relative luminance, 0 for black to 1 for
 * white, or null for anything else. */
export function luminance(color: string): number | null {
  const hex = /^#([0-9a-f]{6})$/i.exec(color)?.[1];
  if (!hex) return null;
  const linear = (i: number) => {
    const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(0) + 0.7152 * linear(2) + 0.0722 * linear(4);
}

/** Whether a background wants dark ink: black contrasts with it more than
 * white does, by WCAG's contrast ratio. Anything unreadable gets white. */
export function isLight(background: string): boolean {
  const l = luminance(background);
  if (l === null) return false;
  return (l + 0.05) / 0.05 > 1.05 / (l + 0.05);
}

/** The stand-in for `glyph`. */
export const fallbackIcon = (glyph: Glyph): LucideIcon => FALLBACK_ICONS[glyph];
