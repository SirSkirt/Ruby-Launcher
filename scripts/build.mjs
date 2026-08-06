import { cp, mkdir, rm } from "node:fs/promises";
import { resolve } from "node:path";

const projectRoot = resolve(import.meta.dirname, "..");
const sourceDir = resolve(projectRoot, "src-web");
const outputDir = resolve(projectRoot, "dist");

await rm(outputDir, { recursive: true, force: true });
await mkdir(outputDir, { recursive: true });
await cp(sourceDir, outputDir, { recursive: true });

console.log(`Ruby Launcher web assets built to ${outputDir}`);
