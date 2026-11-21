import { createHash } from "node:crypto";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { assembleRelease, validateReleaseVersions } from "./release-utils.mjs";

const { cp, mkdir, readFile, writeFile } = vi.hoisted(() => ({
  cp: vi.fn(), mkdir: vi.fn(), readFile: vi.fn(), writeFile: vi.fn(),
}));
vi.mock("node:fs/promises", () => ({ cp, mkdir, readFile, writeFile }));

beforeEach(() => vi.resetAllMocks());

describe("release validation", () => {
  it("accepts matching versions, including zero", () => {
    expect(validateReleaseVersions("v0.1.0", ...Array(5).fill("0.1.0"))).toBe("0.1.0");
    expect(validateReleaseVersions("v12.34.56", ...Array(5).fill("12.34.56"))).toBe("12.34.56");
  });

  it.each(["1.2.3", "v1.2", "v01.2.3", "v1.02.3", "v1.2.03", "v1.2.3-beta", "v1.2.3+build", "v1.2.3\n", undefined])("rejects malformed tag %s", (tag) => {
    expect(() => validateReleaseVersions(tag, ...Array(5).fill("1.2.3"))).toThrow("Release tag must");
  });

  it.each([0, 1, 2, 3, 4])("rejects a mismatch in version field %s", (index) => {
    const versions = Array(5).fill("1.2.3");
    versions[index] = "1.2.2";
    expect(() => validateReleaseVersions("v1.2.3", ...versions)).toThrow("must all match");
  });
});

describe("release assembly", () => {
  it("requires every package and generates checksums for the exact published bytes", async () => {
    readFile.mockResolvedValue(Buffer.from("package"));
    const files = await assembleRelease("1.2.3");
    expect(files).toHaveLength(7);
    expect(files).toContain("agenthub-1.2.3-windows-aarch64.msi");
    expect(files).toContain("agenthub-1.2.3-macos-x86_64.dmg");
    expect(files).toContain("agenthub-1.2.3-debian-amd64.deb");
    expect(mkdir).toHaveBeenCalledWith("release", { recursive: true });
    expect(files).toContain("agenthub-1.2.3-1-arch.src.tar.gz");
    expect(files).not.toContain("agenthub-1.2.3-source.tar.gz");
    expect(readFile).toHaveBeenCalledWith("artifacts/agenthub-1.2.3-1-arch.src.tar.gz");
    expect(readFile).toHaveBeenCalledTimes(7);
    for (const file of files) expect(cp).toHaveBeenCalledWith(expect.any(String), `release/${file}`);
    const checksum = createHash("sha256").update("package").digest("hex");
    expect(writeFile).toHaveBeenCalledTimes(7);
    for (const file of files) {
      expect(writeFile).toHaveBeenCalledWith(`release/${file}.sha256`, `${checksum}  ${file}\n`);
    }
  });

  it("supports alternate directories", async () => {
    readFile.mockResolvedValue(Buffer.from("package"));
    await assembleRelease("1.2.3", "input", "output");
    expect(cp).toHaveBeenCalledWith("input/agenthub-1.2.3-1-arch.src.tar.gz", "output/agenthub-1.2.3-1-arch.src.tar.gz");
    expect(writeFile).toHaveBeenCalledWith("output/agenthub-1.2.3-1-arch.src.tar.gz.sha256", expect.any(String));
  });

  it("rejects an empty package", async () => {
    readFile.mockResolvedValue(Buffer.alloc(0));
    await expect(assembleRelease("1.2.3")).rejects.toThrow("Empty release artifact");
    expect(writeFile).not.toHaveBeenCalled();
  });

  it("rejects missing artifacts rather than publishing a partial release", async () => {
    readFile.mockRejectedValue(new Error("ENOENT"));
    await expect(assembleRelease("1.2.3")).rejects.toThrow("ENOENT");
    expect(writeFile).not.toHaveBeenCalled();
  });
});
