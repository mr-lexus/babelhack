import { spawnSync } from "node:child_process";
import { copyFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../", import.meta.url));
const result = spawnSync(
  process.execPath,
  [
    "node_modules/@tauri-apps/cli/tauri.js",
    "icon",
    "public/brand/app-icon.svg",
    "--output",
    "src-tauri/icons",
  ],
  { cwd: root, stdio: "inherit" },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
copyFileSync(
  new URL("../public/brand/app-icon.svg", import.meta.url),
  new URL("../public/favicon.svg", import.meta.url),
);
