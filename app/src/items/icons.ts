import { schemeUrl } from "@/lib/platform";

/** An icon's URL. The app serves the icons it built out of the user's dump. */
export const iconUrl = (icon: string) => schemeUrl("zootr-icon", `${icon}.png`);
