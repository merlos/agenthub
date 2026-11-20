import { createHash } from "node:crypto";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { execFileSync, cp, mkdir, readFile, writeFile } = vi.hoisted(() => ({
  execFileSync: vi.fn(),
  cp: vi.fn(),
  mkdir: vi.fn(),
  readFile: vi.fn(),
  writeFile: vi.fn(),
}));

vi.mock("node:child_process", () => ({ execFileSync }));
vi.mock("node:fs/promises", () => ({ cp, mkdir, readFile, writeFile }));

describe("Arch source packaging", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.resetAllMocks();
    vi.stubEnv("SOURCE_ARCHIVE", undefined);
    vi.stubEnv("SOURCE_SHA256", undefined);
    vi.stubEnv("AUR_OUTPUT", undefined);
    readFile.mockImplementation(async (file: string) => {
      if (file === "package.json") return '{"version":"0.1.0"}';
      if (file === "packaging/aur/PKGBUILD.template") return "pkgver=@VERSION@\nsha256sums=('@SHA256@')\n";
      return Buffer.from("source");
    });
    execFileSync.mockReturnValue("pkgbase = agenthub\n");
    vi.spyOn(console, "log").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.spyOn(process, "exit").mockImplementation(() => { throw new Error("exit"); });
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  const run = () => import("./render-aur.mjs");

  it("renders metadata and builds a self-contained source archive without publishing", async () => {
    await run();
    const checksum = createHash("sha256").update("source").digest("hex");
    expect(mkdir).toHaveBeenCalledWith("artifacts/aur", { recursive: true });
    expect(writeFile).toHaveBeenCalledWith("artifacts/aur/PKGBUILD", `pkgver=0.1.0\nsha256sums=('${checksum}')\n`);
    expect(cp).toHaveBeenCalledWith("artifacts/agenthub-0.1.0-source.tar.gz", "artifacts/aur/agenthub-0.1.0.tar.gz");
    expect(writeFile).toHaveBeenCalledWith("artifacts/aur/.SRCINFO", "pkgbase = agenthub\n");
    expect(execFileSync.mock.calls).toEqual([
      ["makepkg", ["--printsrcinfo"], { cwd: "artifacts/aur", encoding: "utf8" }],
      ["makepkg", ["--allsource", "--force"], {
        cwd: "artifacts/aur",
        stdio: "inherit",
        env: { ...process.env, SRCDEST: path.resolve("artifacts/aur"), SRCPKGDEST: path.resolve("artifacts/aur"), SRCEXT: ".src.tar.gz" },
      }],
    ]);
    expect(console.log).toHaveBeenCalledWith("artifacts/aur");
  });

  it("honors custom archive, checksum, and output paths", async () => {
    vi.stubEnv("SOURCE_ARCHIVE", "/custom/source.tar.gz");
    vi.stubEnv("SOURCE_SHA256", "custom-checksum");
    vi.stubEnv("AUR_OUTPUT", "/custom/output");
    await run();
    expect(readFile).not.toHaveBeenCalledWith("/custom/source.tar.gz");
    expect(cp).toHaveBeenCalledWith("/custom/source.tar.gz", "/custom/output/agenthub-0.1.0.tar.gz");
    expect(writeFile).toHaveBeenCalledWith("/custom/output/PKGBUILD", "pkgver=0.1.0\nsha256sums=('custom-checksum')\n");
    expect(execFileSync).toHaveBeenLastCalledWith("makepkg", ["--allsource", "--force"], expect.objectContaining({
      cwd: "/custom/output",
      env: expect.objectContaining({ SRCDEST: "/custom/output", SRCPKGDEST: "/custom/output" }),
    }));
  });

  it("reports a missing makepkg and exits unsuccessfully", async () => {
    execFileSync.mockImplementation(() => { throw new Error("ENOENT"); });
    await expect(run()).rejects.toThrow("exit");
    expect(process.exit).toHaveBeenCalledWith(1);
    expect(console.error).toHaveBeenCalledWith(expect.stringContaining("makepkg installed"));
    expect(execFileSync).toHaveBeenCalledTimes(1);
  });

  it("preserves makepkg's failure status when source creation fails", async () => {
    execFileSync.mockImplementationOnce(() => "metadata").mockImplementationOnce(() => {
      throw Object.assign(new Error("checksum mismatch"), { status: 7 });
    });
    await expect(run()).rejects.toThrow("exit");
    expect(process.exit).toHaveBeenCalledWith(7);
  });
});
