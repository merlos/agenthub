import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const workflow = readFileSync(".github/workflows/release.yml", "utf8");
const step = workflow.split("      - name: Build signed and notarized macOS package\n")[1]
  .split("      - name: Verify Windows installer signature\n")[0];
const script = step.split("        run: |\n")[1]
  .split("\n").map((line) => line.slice(10)).join("\n")
  .replace("${{ matrix.target }}", "macos-arm64");
const directories: string[] = [];

afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true });
});

function runSigning(overrides: Record<string, string> = {}) {
  const directory = mkdtempSync(path.join(tmpdir(), "agenthub-signing-test-"));
  directories.push(directory);
  const bin = path.join(directory, "bin");
  const runnerTemp = path.join(directory, "runner");
  mkdirSync(bin);
  mkdirSync(runnerTemp);
  // Use real OpenSSL and a disposable fixture certificate/key, never release secrets.
  const key = path.join(directory, "key.pem");
  const certificate = path.join(directory, "certificate.pem");
  const p12 = path.join(directory, "certificate.p12");
  execFileSync("openssl", [
    "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:P-256",
    "-keyout", key, "-out", certificate, "-nodes", "-days", "1",
    "-subj", "/CN=Developer ID Application: Example (TEAM123)",
  ], { stdio: "ignore" });
  execFileSync("openssl", [
    "pkcs12", "-export", "-inkey", key, "-in", certificate, "-out", p12,
    "-passout", "pass:fixture-password",
  ], { stdio: "ignore" });
  writeFileSync(path.join(bin, "npm"), `#!/bin/bash
set -euo pipefail
test "$*" = "run package -- macos-arm64"
test "$APPLE_SIGNING_IDENTITY" = "Developer ID Application: Example (TEAM123)"
test "$APPLE_API_KEY" = "KEY123"
test "$APPLE_API_ISSUER" = "issuer-id"
test -z "\${APPLE_API_PRIVATE_KEY+x}"
test -s "$APPLE_API_KEY_PATH"
test "$(LC_ALL=C ls -l "$APPLE_API_KEY_PATH" | cut -c 1-10)" = "-rw-------"
printf 'package called'
exit "\${PACKAGE_EXIT_STATUS:-0}"
`, { mode: 0o755 });
  const result = spawnSync("bash", ["-e", "-o", "pipefail", "-c", script], {
    encoding: "utf8",
    env: {
      ...process.env,
      PATH: `${bin}:${process.env.PATH}`,
      RUNNER_TEMP: runnerTemp,
      APPLE_CERTIFICATE: readFileSync(p12).toString("base64"),
      APPLE_CERTIFICATE_PASSWORD: "fixture-password",
      APPLE_API_PRIVATE_KEY: readFileSync(key, "utf8"),
      APPLE_API_KEY: "KEY123",
      APPLE_API_ISSUER: "issuer-id",
      APPLE_TEAM_ID: "TEAM123",
      ...overrides,
    },
  });
  expect(readdirSync(runnerTemp)).toEqual([]);
  return result;
}

describe("macOS release signing", () => {
  it("maps the six repository secrets to Tauri's credentials", () => {
    expect(step).toContain("shell: bash");
    expect(step).toContain("APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE_BASE64 }}");
    expect(step).toContain("APPLE_API_KEY: ${{ secrets.APPLE_API_KEY_ID }}");
    expect(step).toContain("APPLE_API_PRIVATE_KEY: ${{ secrets.APPLE_API_KEY }}");
    for (const name of ["APPLE_CERTIFICATE_PASSWORD", "APPLE_API_ISSUER", "APPLE_TEAM_ID"]) {
      expect(step).toContain(`${name}: \${{ secrets.${name} }}`);
    }
    expect(step).not.toContain("secrets.APPLE_ID");
    expect(step).not.toContain("secrets.APPLE_PASSWORD");
    expect(step).not.toContain("secrets.APPLE_SIGNING_IDENTITY");
  });

  it("derives the identity, supplies a restricted API key file, and cleans up", () => {
    const result = runSigning();
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toBe("package called");
  });

  it("removes temporary credentials even when packaging fails", () => {
    const result = runSigning({ PACKAGE_EXIT_STATUS: "1" });
    expect(result.status).toBe(1);
    expect(result.stdout).toBe("package called");
  });

  it.each([
    "APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_API_ISSUER",
    "APPLE_API_KEY", "APPLE_API_PRIVATE_KEY", "APPLE_TEAM_ID",
  ])("rejects missing %s before building", (name) => {
    const result = runSigning({ [name]: "" });
    expect(result.status).not.toBe(0);
    expect(result.stdout).toContain(`Missing required macOS signing credential: ${name}`);
    expect(result.stdout).not.toContain("package called");
  });

  it.each([
    { APPLE_TEAM_ID: "OTHERTEAM" },
    { APPLE_API_PRIVATE_KEY: "not a PEM key" },
    { APPLE_CERTIFICATE: "not base64" },
    { APPLE_CERTIFICATE: Buffer.from("not a PKCS12 certificate").toString("base64") },
    { APPLE_CERTIFICATE_PASSWORD: "wrong-password" },
  ])("rejects invalid credentials and cleans up: %j", (overrides) => {
    const result = runSigning(overrides);
    expect(result.status).not.toBe(0);
    expect(result.stdout).not.toContain("package called");
  });
});
