import { cp, mkdir, readdir, readFile, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";
import process from "node:process";

const targets = {
  "windows-x64": { os: "win32", arch: "x64", triple: "x86_64-pc-windows-msvc", bundle: "msi", extension: ".msi", outputArch: "x86_64" },
  "windows-arm64": { os: "win32", arch: "arm64", triple: "aarch64-pc-windows-msvc", bundle: "msi", extension: ".msi", outputArch: "aarch64" },
  "macos-x64": { os: "darwin", arch: "x64", triple: "x86_64-apple-darwin", bundle: "dmg", extension: ".dmg", outputArch: "x86_64" },
  "macos-arm64": { os: "darwin", arch: "arm64", triple: "aarch64-apple-darwin", bundle: "dmg", extension: ".dmg", outputArch: "aarch64" },
  "debian-x64": { os: "linux", arch: "x64", triple: "x86_64-unknown-linux-gnu", bundle: "deb", extension: ".deb", outputArch: "amd64" },
  "debian-arm64": { os: "linux", arch: "arm64", triple: "aarch64-unknown-linux-gnu", bundle: "deb", extension: ".deb", outputArch: "arm64" },
};

function fail(message) {
  console.error(`packaging error: ${message}`);
  process.exit(1);
}

async function versions() {
  const npm = JSON.parse(await readFile("package.json", "utf8")).version;
  const tauri = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8")).version;
  const cargoText = await readFile("src-tauri/Cargo.toml", "utf8");
  const cargo = cargoText.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if (!cargo || npm !== tauri || npm !== cargo) fail(`version mismatch: npm=${npm}, Tauri=${tauri}, Cargo=${cargo ?? "missing"}`);
  const expectedTag = process.env.RELEASE_TAG;
  if (expectedTag && expectedTag !== `v${npm}`) fail(`tag ${expectedTag} does not match application version v${npm}`);
  return npm;
}

async function findFiles(directory, extension) {
  const matches = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) matches.push(...await findFiles(file, extension));
    else if (entry.name.toLowerCase().endsWith(extension)) matches.push(file);
  }
  return matches;
}

const name = process.argv[2];
if (!name || !targets[name]) fail(`usage: npm run package -- <${Object.keys(targets).join("|")}>`);
const target = targets[name];
if (process.platform !== target.os || process.arch !== target.arch) {
  fail(`${name} must run on ${target.os}/${target.arch}; current host is ${process.platform}/${process.arch}`);
}

const version = await versions();
// Run the CLI with Node directly: spawning npm.cmd without a shell fails on Windows.
const cli = createRequire(import.meta.url).resolve("@tauri-apps/cli/tauri.js");
const args = [cli, "build", "--ci", "--bundles", target.bundle, "--target", target.triple];
if (process.env.TAURI_BUILD_CONFIG) args.push("--config", process.env.TAURI_BUILD_CONFIG);
args.push("--", "--locked");
const result = spawnSync(process.execPath, args, { stdio: "inherit", env: process.env });
if (result.status !== 0) process.exit(result.status ?? 1);

const bundleRoot = path.join("src-tauri", "target", target.triple, "release", "bundle");
const packages = await findFiles(bundleRoot, target.extension).catch(() => []);
if (packages.length !== 1) fail(`expected one ${target.extension} under ${bundleRoot}, found ${packages.length}`);
await mkdir("artifacts", { recursive: true });
const destination = path.join("artifacts", `agenthub-${version}-${name.replace("x64", target.outputArch).replace("arm64", target.outputArch)}${target.extension}`);
await rm(destination, { force: true });
await cp(packages[0], destination);
console.log(destination);
