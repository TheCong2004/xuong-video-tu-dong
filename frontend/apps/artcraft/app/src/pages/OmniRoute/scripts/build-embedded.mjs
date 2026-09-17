#!/usr/bin/env node

import { spawn } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";

const projectRoot = process.cwd();
const nextBin = path.join(projectRoot, "node_modules", "next", "dist", "bin", "next");
const buildHome = path.join(os.tmpdir(), `artcraft-omniroute-build-${process.pid}`);
const env = {
  ...process.env,
  NEXT_PRIVATE_BUILD_WORKER: "0",
  NODE_OPTIONS: process.env.NODE_OPTIONS?.includes("--max-old-space-size")
    ? process.env.NODE_OPTIONS
    : `${process.env.NODE_OPTIONS ?? ""} --max-old-space-size=10240`.trim(),
};

function packageDirectoryFromEntry(entryPath, packageName) {
  let current = path.dirname(entryPath);
  while (current !== path.dirname(current)) {
    const manifest = path.join(current, "package.json");
    if (existsSync(manifest)) {
      try {
        if (JSON.parse(readFileSync(manifest, "utf8")).name === packageName) return current;
      } catch {
        // Keep walking when an unrelated package manifest is malformed.
      }
    }
    current = path.dirname(current);
  }
  return null;
}

function resolveDependency(packageName, sourceDirectory) {
  const packageRequire = createRequire(path.join(realpathSync(sourceDirectory), "package.json"));
  try {
    return path.dirname(packageRequire.resolve(`${packageName}/package.json`));
  } catch {
    try {
      return packageDirectoryFromEntry(packageRequire.resolve(packageName), packageName);
    } catch {
      return null;
    }
  }
}

function prepareStandalone() {
  const standaloneRoot = path.join(projectRoot, ".build", "next", "standalone");
  const standaloneModules = path.join(standaloneRoot, "node_modules");
  const standalonePackage = path.join(standaloneRoot, "package.json");
  if (!existsSync(standalonePackage) || !existsSync(standaloneModules)) {
    throw new Error("Embedded OmniRoute standalone output is missing; run the build first.");
  }

  const packageJson = JSON.parse(readFileSync(standalonePackage, "utf8"));
  packageJson.type = "commonjs";
  writeFileSync(standalonePackage, `${JSON.stringify(packageJson, null, 2)}\n`);

  // pnpm stores dependencies beside each package and exposes them through
  // junctions. Next copies package contents into standalone, but those sibling
  // junctions are not portable. Materialize the runtime dependency closure at
  // the standalone node_modules root so the installed app has no link back to
  // this source tree.
  const roots = [];
  for (const entry of readdirSync(standaloneModules, { withFileTypes: true })) {
    if ((!entry.isDirectory() && !entry.isSymbolicLink()) || entry.name === ".pnpm") continue;
    if (entry.name.startsWith("@")) {
      const scopeDirectory = path.join(standaloneModules, entry.name);
      for (const child of readdirSync(scopeDirectory, { withFileTypes: true })) {
        if (child.isDirectory() || child.isSymbolicLink()) roots.push(`${entry.name}/${child.name}`);
      }
    } else {
      roots.push(entry.name);
    }
  }

  const queue = [];
  for (const packageName of roots) {
    const sourceDirectory = path.join(projectRoot, "node_modules", ...packageName.split("/"));
    if (existsSync(path.join(sourceDirectory, "package.json"))) {
      queue.push({ packageName, sourceDirectory });
    }
  }

  const visited = new Set();
  while (queue.length > 0) {
    const { packageName, sourceDirectory } = queue.shift();
    if (visited.has(packageName)) continue;
    visited.add(packageName);

    const destination = path.join(standaloneModules, ...packageName.split("/"));
    if (!existsSync(path.join(destination, "package.json"))) {
      mkdirSync(path.dirname(destination), { recursive: true });
      cpSync(realpathSync(sourceDirectory), destination, {
        recursive: true,
        dereference: true,
        force: true,
      });
    }

    const manifest = JSON.parse(readFileSync(path.join(sourceDirectory, "package.json"), "utf8"));
    const dependencyNames = new Set([
      ...Object.keys(manifest.dependencies ?? {}),
      ...Object.keys(manifest.optionalDependencies ?? {}),
      ...Object.keys(manifest.peerDependencies ?? {}),
    ]);
    for (const dependencyName of dependencyNames) {
      if (visited.has(dependencyName)) continue;
      const dependencyDirectory = resolveDependency(dependencyName, sourceDirectory);
      if (dependencyDirectory) {
        queue.push({ packageName: dependencyName, sourceDirectory: dependencyDirectory });
      } else if (manifest.dependencies?.[dependencyName]) {
        throw new Error(`Cannot resolve runtime dependency ${dependencyName} required by ${packageName}`);
      }
    }
  }
}

if (process.argv.includes("--prepare-existing")) {
  prepareStandalone();
  process.exit(0);
}

if (process.platform === "win32") {
  env.HOME = buildHome;
  env.USERPROFILE = buildHome;
  env.APPDATA = path.join(buildHome, "AppData", "Roaming");
  env.LOCALAPPDATA = path.join(buildHome, "AppData", "Local");
  mkdirSync(env.APPDATA, { recursive: true });
  mkdirSync(env.LOCALAPPDATA, { recursive: true });
}

const child = spawn(process.execPath, [nextBin, "build", "--webpack"], {
  cwd: projectRoot,
  env,
  stdio: "inherit",
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => child.kill(signal));
}

child.on("exit", (code, signal) => {
  if (process.platform === "win32") {
    rmSync(buildHome, { recursive: true, force: true });
  }
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  if (code === 0) {
    prepareStandalone();
  }
  process.exit(code ?? 1);
});
