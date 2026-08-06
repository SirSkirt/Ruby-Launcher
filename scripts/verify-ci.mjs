import { access, readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const packageJson = JSON.parse(await readFile(resolve(root, "package.json"), "utf8"));
const tauriConfig = JSON.parse(await readFile(resolve(root, "src-tauri", "tauri.conf.json"), "utf8"));
const cargoToml = await readFile(resolve(root, "src-tauri", "Cargo.toml"), "utf8");
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const versions = new Map([
  ["package.json", packageJson.version],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
  ["src-tauri/Cargo.toml", cargoVersion],
]);

const expected = packageJson.version;
const mismatches = [...versions].filter(([, value]) => value !== expected);
if (mismatches.length) {
  throw new Error(`Ruby version mismatch: ${[...versions].map(([file, value]) => `${file}=${value}`).join(", ")}`);
}

for (const path of [
  "src-web/index.html",
  "src-web/app.js",
  "src-web/styles.css",
  "module-loader/src",
  "modules/controller/src",
  "src-tauri/icons/icon.ico",
]) {
  await access(resolve(root, path));
}

if (packageJson.devDependencies?.["@tauri-apps/cli"]?.startsWith("^") || packageJson.devDependencies?.["@tauri-apps/cli"]?.startsWith("~")) {
  throw new Error("@tauri-apps/cli must be pinned to an exact version for GitHub Actions builds.");
}

console.log(`Ruby Launcher CI verification passed for v${expected}.`);
