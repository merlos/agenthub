import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { cp, mkdir, readFile, writeFile } = vi.hoisted(() => ({
  cp: vi.fn(), mkdir: vi.fn(), readFile: vi.fn(), writeFile: vi.fn(),
}));
vi.mock("node:fs/promises", () => ({ cp, mkdir, readFile, writeFile }));

const manifests: Record<string, string> = {
  "package.json": '{"version":"1.2.3"}',
  "src-tauri/tauri.conf.json": '{"version":"1.2.3"}',
  "src-tauri/Cargo.toml": '[package]\nversion = "1.2.3"\n',
  "package-lock.json": '{"packages":{"":{"version":"1.2.3"}}}',
  "src-tauri/Cargo.lock": '[[package]]\nname = "agenthub"\nversion = "1.2.3"\n',
};

beforeEach(() => {
  vi.resetModules();
  vi.resetAllMocks();
  vi.stubEnv("RELEASE_TAG", "v1.2.3");
  readFile.mockImplementation(async (file: string) => manifests[file] ?? Buffer.from("artifact"));
  vi.spyOn(console, "log").mockImplementation(() => {});
});

afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

describe("release entrypoints", () => {
  it("reads and checks the actual manifests and lockfiles", async () => {
    await import("./check-release.mjs");
    expect(console.log).toHaveBeenCalledWith("1.2.3");
    expect(readFile).toHaveBeenCalledTimes(5);
  });

  it("accepts Windows lockfile line endings", async () => {
    readFile.mockImplementation(async (file: string) => manifests[file].replaceAll("\n", "\r\n"));
    await import("./check-release.mjs");
    expect(console.log).toHaveBeenCalledWith("1.2.3");
  });

  it.each(["src-tauri/Cargo.toml", "src-tauri/Cargo.lock"])("rejects a missing application version in %s", async (missing) => {
    readFile.mockImplementation(async (file: string) => file === missing ? "" : manifests[file]);
    await expect(import("./check-release.mjs")).rejects.toThrow("must all match");
  });

  it("assembles the versioned downloaded packages", async () => {
    await import("./prepare-release.mjs");
    expect(cp).toHaveBeenCalledTimes(7);
    expect(cp).toHaveBeenCalledWith("artifacts/agenthub-1.2.3-windows-aarch64.msi", "release/agenthub-1.2.3-windows-aarch64.msi");
    expect(writeFile).toHaveBeenCalledTimes(7);
    expect(writeFile).toHaveBeenCalledWith("release/agenthub-1.2.3-windows-aarch64.msi.sha256", expect.any(String));
  });
});
