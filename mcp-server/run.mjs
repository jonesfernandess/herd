import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const serverDir = path.dirname(fileURLToPath(import.meta.url));
const srcDir = path.join(serverDir, "src");
const distEntry = path.join(serverDir, "dist", "index.js");
const buildInputs = [
  path.join(serverDir, "package.json"),
  path.join(serverDir, "tsconfig.json"),
  srcDir,
];

function newestMtimeMs(targetPath) {
  if (!existsSync(targetPath)) {
    return 0;
  }
  const stat = statSync(targetPath);
  if (!stat.isDirectory()) {
    return stat.mtimeMs;
  }
  let newest = stat.mtimeMs;
  for (const entry of readdirSync(targetPath)) {
    newest = Math.max(newest, newestMtimeMs(path.join(targetPath, entry)));
  }
  return newest;
}

function ensureBuilt() {
  const distMtime = newestMtimeMs(distEntry);
  const sourceMtime = Math.max(...buildInputs.map((targetPath) => newestMtimeMs(targetPath)));
  if (distMtime >= sourceMtime) {
    return;
  }

  const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";
  const result = spawnSync(npmCommand, ["run", "build"], {
    cwd: serverDir,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

ensureBuilt();
const mod = await import(pathToFileURL(distEntry).href);
await mod.main();
