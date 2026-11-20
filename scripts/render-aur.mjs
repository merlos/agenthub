import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const version = JSON.parse(await readFile("package.json", "utf8")).version;
const archive = process.env.SOURCE_ARCHIVE ?? `artifacts/agenthub-${version}-source.tar.gz`;
const checksum = process.env.SOURCE_SHA256 ?? createHash("sha256").update(await readFile(archive)).digest("hex");
const template = await readFile("packaging/aur/PKGBUILD.template", "utf8");
const output = process.env.AUR_OUTPUT ?? "artifacts/aur";
await mkdir(output, { recursive: true });
await writeFile(path.join(output, "PKGBUILD"), template.replaceAll("@VERSION@", version).replaceAll("@SHA256@", checksum));
// Seed makepkg's source cache so an unpublished release can be packaged locally.
await cp(archive, path.join(output, `agenthub-${version}.tar.gz`));
try {
  const srcinfo = execFileSync("makepkg", ["--printsrcinfo"], { cwd: output, encoding: "utf8" });
  await writeFile(path.join(output, ".SRCINFO"), srcinfo);
  execFileSync("makepkg", ["--allsource", "--force"], {
    cwd: output,
    stdio: "inherit",
    env: { ...process.env, SRCDEST: path.resolve(output), SRCPKGDEST: path.resolve(output), SRCEXT: ".src.tar.gz" },
  });
} catch (error) {
  console.error("Arch source packaging failed. Run on Arch Linux with makepkg installed; check its output above.");
  process.exit(error.status ?? 1);
}
console.log(output);
