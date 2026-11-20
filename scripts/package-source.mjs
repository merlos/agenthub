import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import process from "node:process";

const version = JSON.parse(await readFile("package.json", "utf8")).version;
if (process.env.RELEASE_TAG && process.env.RELEASE_TAG !== `v${version}`) {
  throw new Error(`tag ${process.env.RELEASE_TAG} does not match v${version}`);
}
await mkdir("artifacts", { recursive: true });
const output = `artifacts/agenthub-${version}-source.tar.gz`;
const archive = spawnSync("git", ["archive", "--format=tar.gz", `--prefix=agenthub-${version}/`, "HEAD", "-o", output], { stdio: "inherit" });
if (archive.status !== 0) process.exit(archive.status ?? 1);
const digest = createHash("sha256").update(await readFile(output)).digest("hex");
await writeFile(`${output}.sha256`, `${digest}  agenthub-${version}-source.tar.gz\n`);
console.log(`${output}\n${digest}`);
