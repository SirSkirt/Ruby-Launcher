import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const loaderSource = join(root, "module-loader", "src");
const loaderClasses = join(root, "module-loader", "build", "classes");
const loaderManifest = join(root, "module-loader", "MANIFEST.MF");
const loaderOutput = join(root, "src-tauri", "resources", "ruby-module-loader.jar");

const controllerSource = join(root, "modules", "controller", "src");
const controllerResources = join(root, "modules", "controller", "resources");
const controllerClasses = join(root, "modules", "controller", "build", "classes");
const controllerOutput = join(root, "src-tauri", "resources", "modules", "ruby-controller.rmod");

function javaSources(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? javaSources(path) : entry.name.endsWith(".java") ? [path] : [];
  });
}

function compile(sourceRoot, classes, extraArgs = [], useRelease = true) {
  rmSync(classes, { recursive: true, force: true });
  mkdirSync(classes, { recursive: true });
  const sources = javaSources(sourceRoot);
  if (!sources.length) throw new Error(`No Java sources found in ${sourceRoot}`);
  const targetArgs = useRelease
    ? ["--release", "17"]
    : ["-source", "17", "-target", "17"];
  execFileSync("javac", [...targetArgs, ...extraArgs, "-d", classes, ...sources], { stdio: "inherit" });
}

mkdirSync(join(root, "src-tauri", "resources", "modules"), { recursive: true });
compile(loaderSource, loaderClasses, [
  "--add-exports",
  "java.base/jdk.internal.org.objectweb.asm=ALL-UNNAMED"
], false);
execFileSync("jar", ["cfm", loaderOutput, loaderManifest, "-C", loaderClasses, "."], { stdio: "inherit" });

compile(controllerSource, controllerClasses, ["-cp", loaderClasses]);
execFileSync("jar", ["cf", controllerOutput, "-C", controllerClasses, ".", "-C", controllerResources, "."], { stdio: "inherit" });

console.log(`Ruby Module Loader built to ${loaderOutput}`);
console.log(`Controller Support module built to ${controllerOutput}`);
