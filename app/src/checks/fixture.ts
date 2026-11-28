import type { Row } from "@/bindings";

/** A row for tests, unchecked and unmarked unless `patch` says. */
export const row = (
  hash: string,
  area: string,
  name: string,
  patch: Partial<Row> = {},
): Row => ({
  spot: {
    hash,
    area,
    holder: "Obj_A",
    vanilla: "Item_A",
    item: "Item_B",
    area_name: area,
    chest_name: name,
    item_name: `Item in ${name}`,
  },
  checked: false,
  tracked: true,
  flag: 1,
  label: null,
  mark: null,
  ...patch,
});
