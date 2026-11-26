import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { cp, mkdir, readdir, readFile, rm, spawnSync } = vi.hoisted(() => ({
  cp: vi.fn(), mkdir: vi.fn(), readdir: vi.fn(), readFile: vi.fn(), rm: vi.fn(), spawnSync: vi.fn(),
}));
vi.mock("node:fs/promises", () => ({ cp, mkdir, readdir, readFile, rm }));
vi.mock("node:child_process", () => ({ spawnSync }));
vi.mock("node:module", () => ({ createRequire: () => ({ resolve: () => "/cli/tauri.js" }) }));

const originalPlatform = process.platform;
const originalArch = process.arch;
const originalArgv = process.argv;
const file = (name: string, directory = false) => ({ name, isDirectory: () => directory });
const run = () => import("./package.mjs");
const host = (platform: string, arch: string) => {
  Object.defineProperty(process, "platform", { value: platform });
  Object.defineProperty(process, "arch", { value: arch });
};

beforeEach(() => {
  vi.resetModules();
  vi.resetAllMocks();
  host("linux", "x64");
  process.argv = ["node", "package.mjs", "debian-x64"];
  vi.stubEnv("RELEASE_TAG", undefined);
  vi.stubEnv("TAURI_BUILD_CONFIG", undefined);
  readFile.mockImplementation(async (file: string) => file.endsWith("Cargo.toml") ? 'version = "1.2.3"' : '{"version":"1.2.3"}');
  readdir.mockResolvedValue([file("agenthub.deb")]);
  spawnSync.mockReturnValue({ status: 0 });
  vi.spyOn(console, "log").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.spyOn(process, "exit").mockImplementation(() => { throw new Error("exit"); });
});

afterEach(() => {
  host(originalPlatform, originalArch);
  process.argv = originalArgv;
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe("binary packaging", () => {
  it.each([
    ["windows-x64", "win32", "x64", "x86_64-pc-windows-msvc", "msi", "windows-x86_64"],
    ["windows-arm64", "win32", "arm64", "aarch64-pc-windows-msvc", "msi", "windows-aarch64"],
    ["macos-x64", "darwin", "x64", "x86_64-apple-darwin", "dmg", "macos-x86_64"],
    ["macos-arm64", "darwin", "arm64", "aarch64-apple-darwin", "dmg", "macos-aarch64"],
    ["debian-x64", "linux", "x64", "x86_64-unknown-linux-gnu", "deb", "debian-amd64"],
    ["debian-arm64", "linux", "arm64", "aarch64-unknown-linux-gnu", "deb", "debian-arm64"],
  ])("builds %s natively with Node, a locked Cargo build, and stable artifact naming", async (target, platform, arch, triple, bundle, suffix) => {
    process.argv[2] = target;
    host(platform, arch);
    readdir.mockResolvedValue([file(`agenthub.${bundle}`)]);
    await run();
    expect(spawnSync).toHaveBeenCalledWith(process.execPath, [
      "/cli/tauri.js", "build", "--ci", "--bundles", platform === "darwin" ? "app,dmg" : bundle, "--target", triple, "--", "--locked",
    ], { stdio: "inherit", env: process.env });
    expect(cp).toHaveBeenCalledWith(path.join("src-tauri", "target", triple, "release", "bundle", `agenthub.${bundle}`), `artifacts/agenthub-1.2.3-${suffix}.${bundle}`);
    expect(rm).toHaveBeenCalledWith(`artifacts/agenthub-1.2.3-${suffix}.${bundle}`, { force: true });
  });

  it("passes signing configuration and finds nested bundles case-insensitively", async () => {
    vi.stubEnv("TAURI_BUILD_CONFIG", '{"bundle":{"windows":{"certificateThumbprint":"test"}}}');
    vi.stubEnv("RELEASE_TAG", "v1.2.3");
    readdir.mockResolvedValueOnce([file("other.txt"), file("deb", true)]).mockResolvedValueOnce([file("agenthub.DEB")]);
    await run();
    expect(spawnSync.mock.calls[0][1]).toContain("--config");
    expect(spawnSync.mock.calls[0][1]).toContain(process.env.TAURI_BUILD_CONFIG);
    expect(cp.mock.calls[0][0]).toContain("/deb/agenthub.DEB");
  });

  it.each([undefined, "unknown"])("rejects invalid target %s", async (target) => {
    process.argv = target ? ["node", "package.mjs", target] : ["node", "package.mjs"];
    await expect(run()).rejects.toThrow("exit");
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining("usage: npm run package -- <"));
  });

  it.each([["darwin", "x64"], ["linux", "arm64"]])("rejects the wrong host %s/%s", async (platform, arch) => {
    host(platform, arch);
    await expect(run()).rejects.toThrow("exit");
    expect(spawnSync).not.toHaveBeenCalled();
  });

  it.each(["missing-cargo", "wrong-tauri", "wrong-cargo"])("rejects version mismatch %s", async (reason) => {
    readFile.mockImplementation(async (file: string) => {
      if (file.endsWith("Cargo.toml")) return reason === "missing-cargo" ? "" : `version = "${reason === "wrong-cargo" ? "1.2.2" : "1.2.3"}"`;
      return JSON.stringify({ version: reason === "wrong-tauri" && file.endsWith("tauri.conf.json") ? "1.2.2" : "1.2.3" });
    });
    await expect(run()).rejects.toThrow("exit");
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining("version mismatch"));
  });

  it("rejects a mismatched tag", async () => {
    vi.stubEnv("RELEASE_TAG", "v1.2.2");
    await expect(run()).rejects.toThrow("exit");
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining("tag v1.2.2"));
  });

  it.each([7, null])("propagates failed CLI status %s", async (status) => {
    spawnSync.mockReturnValue({ status });
    await expect(run()).rejects.toThrow("exit");
    expect(process.exit).toHaveBeenCalledWith(status ?? 1);
  });

  it.each(["missing-directory", "zero", "multiple"])("rejects missing/ambiguous output: %s", async (reason) => {
    if (reason === "missing-directory") readdir.mockRejectedValue(new Error("ENOENT"));
    else readdir.mockResolvedValue(reason === "zero" ? [] : [file("one.deb"), file("two.deb")]);
    await expect(run()).rejects.toThrow("exit");
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining("expected one .deb"));
    expect(cp).not.toHaveBeenCalled();
  });
});
