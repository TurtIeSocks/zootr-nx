import type { DoorRow } from "@/bindings";

/** A dungeon's name, by id. The doors' ids are every dungeon one can lead
 * to, each named by its vanilla dungeon. */
export const dungeonName = (doors: DoorRow[], id: string) =>
  doors.find((d) => d.id === id)?.vanilla ?? id;
