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
  ScrollText,
  Shield,
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

/** Each Lucide icon with the icon names it stands in for. Tiers share one:
 * Hookshot and Longshot, the three wallets. */
const STAND_INS: [LucideIcon, string[]][] = [
  [
    WandSparkles,
    [
      "Item_DekuStick",
      "Item_StickBag_Small",
      "Item_StickBag_Medium",
      "Item_StickBag_Large",
    ],
  ],
  [
    Nut,
    [
      "Item_DekuNut",
      "Item_NutBag_Small",
      "Item_NutBag_Medium",
      "Item_NutBag_Large",
    ],
  ],
  [Bomb, ["Item_Bomb"]],
  [Rat, ["Item_Bombchu"]],
  [Target, ["Weapon_Range_Bow"]],
  [Crosshair, ["Weapon_Range_SlingShot"]],
  [Music, ["Item_Ocarina_Fairy", "Item_Ocarina_Time"]],
  [Anchor, ["Item_Hookshot", "Item_Longshot"]],
  [Redo2, ["Item_Boomerang"]],
  [Bean, ["Item_MagicBean"]],
  [Glasses, ["Armor_GlassTruth"]],
  [Hammer, ["Item_Hammer"]],
  [Flame, ["Item_Arrow_Fire", "Item_Medal_Fire"]],
  [Snowflake, ["Item_Arrow_Ice"]],
  [Sun, ["Item_Arrow_Light", "Item_Medal_Light"]],
  [Zap, ["Item_DinFire"]],
  [Wind, ["Item_FaroreWind"]],
  [Diamond, ["Item_NayruLove"]],
  [FlaskRound, ["Item_Bottle"]],
  [Egg, ["Item_WeirdEgg", "Item_PocketEgg"]],
  [Bird, ["Item_AlarmCucco", "Item_PocketCucco", "Item_CojiroCucco"]],
  [Mail, ["Item_ZeldaLetter"]],
  [
    Drama,
    [
      "Armor_Child_Mask_Keaton",
      "Armor_Child_Mask_Skull",
      "Armor_Child_Mask_Spooky",
      "Armor_Child_Mask_Bunny",
    ],
  ],
  [Eye, ["Armor_Child_Mask_Truth"]],
  [Sprout, ["Item_OddMushroom", "Item_OddPowder"]],
  [Axe, ["Item_PoacherSaw"]],
  [
    Sword,
    [
      "Item_BrokenSword",
      "Weapon_Sword_Kokiri",
      "Weapon_Sword_Master",
      "Weapon_Sword_Giant",
      "Weapon_Sword_Biggoron",
    ],
  ],
  [ScrollText, ["Item_Prescription"]],
  [Bug, ["Item_EyeballFrog"]],
  [Droplets, ["Item_EyeDrop"]],
  [Receipt, ["Item_ClaimCheck"]],
  [
    Shield,
    ["Weapon_Shield_Deku", "Weapon_Shield_Hylian", "Weapon_Shield_Mirror"],
  ],
  [Shirt, ["Armor_Adult_Normal", "Armor_Adult_Goron", "Armor_Adult_Zora"]],
  [Footprints, ["Armor_Adult_HeavyBoot"]],
  [Feather, ["Armor_Adult_HoverBoot"]],
  [
    Hand,
    [
      "Armor_Child_Gauntlet_Goron",
      "Armor_Adult_Gauntlet_Silver",
      "Armor_Adult_Gauntlet_Gold",
    ],
  ],
  [Waves, ["Item_Scale_Silver", "Item_Scale_Gold"]],
  [Wallet, ["Item_Wallet_Small", "Item_Wallet_Medium", "Item_Wallet_Large"]],
  [
    Backpack,
    ["Item_BombBag_Small", "Item_BombBag_Medium", "Item_BombBag_Large"],
  ],
  [Package, ["Item_Quiver_Small", "Item_Quiver_Medium", "Item_Quiver_Large"]],
  [
    ShoppingBag,
    ["Item_BulletBag_Small", "Item_BulletBag_Medium", "Item_BulletBag_Large"],
  ],
  [Heart, ["Obj_Heart_Piece"]],
  [Sparkles, ["Item_FairyBless_MP", "Item_FairyBless_MPDouble"]],
  [Ticket, ["Item_Token_Gerudo"]],
  [Vibrate, ["Item_AgonyStone"]],
  [Skull, ["Item_Token_Gold"]],
  [
    Music2,
    [
      "Item_Melody_ZeldaLullaby",
      "Item_Melody_EponaSong",
      "Item_Melody_SariaSong",
      "Item_Melody_SunSong",
      "Item_Melody_TimeSong",
      "Item_Melody_StormSong",
      "Item_Melody_ForestMinuet",
      "Item_Melody_FireBolero",
      "Item_Melody_WaterSerenade",
      "Item_Melody_SpiritRequiem",
      "Item_Melody_ShadowNocturne",
      "Item_Melody_LightPrelude",
    ],
  ],
  [Gem, ["Item_Stone_Forest", "Item_Stone_Fire", "Item_Stone_Water"]],
  [TreePine, ["Item_Medal_Forest"]],
  [Droplet, ["Item_Medal_Water"]],
  [Hourglass, ["Item_Medal_Spirit"]],
  [Moon, ["Item_Medal_Shadow"]],
  [Key, ["Item_Dungeon_SmallKey"]],
  [KeyRound, ["Item_Dungeon_BossKey"]],
  [MapIcon, ["Item_Dungeon_Map"]],
  [Compass, ["Item_Dungeon_Compass"]],
];

/** zootr-nx's own icon for each icon name a cell can carry. The app ships
 * none of the game's art, so these draw until game assets are on. */
export const FALLBACK_ICONS: Record<string, LucideIcon> = Object.fromEntries(
  STAND_INS.flatMap(([icon, names]) => names.map((n) => [n, icon])),
);

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

/** The stand-in for `name`, or a question mark for one it doesn't know. */
export const fallbackIcon = (name: string): LucideIcon =>
  FALLBACK_ICONS[name] ?? CircleHelp;
