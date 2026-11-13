import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const inputs = [
  "package.json",
  "package-lock.json",
  "index.html",
  "tsconfig.json",
  "vite.config.ts",
  "scripts",
  "src",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "src-tauri/build.rs",
  "src-tauri/tauri.conf.json",
  "src-tauri/src",
  "src-tauri/assets",
  "src-tauri/icons",
  "src-tauri/capabilities",
];
const files = [];
function collect(path) {
  if (!existsSync(path)) return;
  if (statSync(path).isDirectory()) {
    for (const entry of readdirSync(path).sort()) collect(join(path, entry));
  } else {
    files.push(path);
  }
}
for (const input of inputs) collect(join(root, input));
const hash = createHash("sha256");
for (const file of files.sort()) {
  hash.update(relative(root, file).replaceAll("\\", "/"));
  hash.update("\0");
  hash.update(readFileSync(file));
  hash.update("\0");
}
console.log(hash.digest("hex"));
