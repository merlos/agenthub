import { access, copyFile, mkdir } from "node:fs/promises";
import { spawnSync } from "node:child_process";

const directory = "docs/assets";
const names = ["overview", "applications", "providers", "openai-statistics", "request-logs", "settings"];
const frames = names.map(name => `${directory}/${name}.png`);
const websiteDirectory = "website/public/screenshots";

await mkdir(directory, { recursive: true });
await mkdir(websiteDirectory, { recursive: true });
for (const frame of frames) {
  try {
    await access(frame);
  } catch {
    console.error(`Missing ${frame}. Run \`npm run screenshots\` first.`);
    process.exit(1);
  }
  await copyFile(frame, `${websiteDirectory}/${frame.split("/").at(-1)}`);
}

const result = spawnSync("magick", [
  "-delay", "300",
  ...frames,
  "-resize", "960x",
  "-loop", "0",
  "-layers", "Optimize",
  `${directory}/agenthub-demo.gif`,
], { stdio: "inherit" });

if (result.error?.code === "ENOENT") {
  console.error("ImageMagick is required. Install it and ensure `magick` is on PATH.");
  process.exit(1);
}

process.exit(result.status ?? 1);
