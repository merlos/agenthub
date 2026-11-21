import { createHash } from "node:crypto";
import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

export function validateReleaseVersions(tag, npm, tauri, cargo, npmLock, cargoLock) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag)) {
    throw new Error("Release tag must be vX.Y.Z (no prerelease suffix or leading zeros).");
  }
  const version = tag.slice(1);
  if ([npm, tauri, cargo, npmLock, cargoLock].some((value) => value !== version)) {
    throw new Error("Release tag, npm/Tauri/Cargo versions, and lockfiles must all match.");
  }
  return version;
}

export async function assembleRelease(version, input = "artifacts", output = "release") {
  const files = [
    `agenthub-${version}-windows-x86_64.msi`,
    `agenthub-${version}-windows-aarch64.msi`,
    `agenthub-${version}-macos-x86_64.dmg`,
    `agenthub-${version}-macos-aarch64.dmg`,
    `agenthub-${version}-debian-amd64.deb`,
    `agenthub-${version}-debian-arm64.deb`,
    `agenthub-${version}-1-arch.src.tar.gz`,
  ];
  await mkdir(output, { recursive: true });
  for (const file of files) {
    const source = path.join(input, file);
    const bytes = await readFile(source);
    if (bytes.length === 0) throw new Error(`Empty release artifact: ${file}`);
    await cp(source, path.join(output, file));
    await writeFile(path.join(output, `${file}.sha256`), `${createHash("sha256").update(bytes).digest("hex")}  ${file}\n`);
  }
  return files;
}
