import { readFile } from "node:fs/promises";
import { validateReleaseVersions } from "./release-utils.mjs";

const npm = JSON.parse(await readFile("package.json", "utf8"));
const tauri = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
const cargo = await readFile("src-tauri/Cargo.toml", "utf8");
const npmLock = JSON.parse(await readFile("package-lock.json", "utf8"));
const cargoLock = await readFile("src-tauri/Cargo.lock", "utf8");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const cargoLockVersion = cargoLock.match(/\[\[package\]\]\r?\nname = "agenthub"\r?\nversion = "([^"]+)"/)?.[1];
console.log(validateReleaseVersions(process.env.RELEASE_TAG, npm.version, tauri.version, cargoVersion, npmLock.packages[""].version, cargoLockVersion));
