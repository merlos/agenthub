import { readFile } from "node:fs/promises";
import { assembleRelease } from "./release-utils.mjs";

const { version } = JSON.parse(await readFile("package.json", "utf8"));
await assembleRelease(version);
