import { readFileSync } from "node:fs";
import { join } from "node:path";

/** The zootr crate's sources, from the app folder tests run in. */
const SRC = join(process.cwd(), "..", "crates", "zootr", "src");

/** A source file of the zootr crate, without its tests, for tests that
 * hold the app to what the Rust code does. */
export const rust = (path: string) =>
  readFileSync(join(SRC, path), "utf8").split("#[cfg(test)]")[0] ?? "";
