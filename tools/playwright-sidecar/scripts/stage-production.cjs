#!/usr/bin/env node
// Stage the immutable runtime inputs used by the packaged Tauri app.
// Required environment variables:
//   FLOWORD_NODE_RUNTIME       path to node.exe
//   FLOWORD_CHROMIUM_DIR       Playwright Chromium directory
//   FLOWORD_CHROMEX_EXTENSION  unpacked extension directory
//   FLOWORD_DONUT_RUNTIME_EXE  floword-donut-runtime.exe

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const childProcess = require('node:child_process');

const repo = path.resolve(__dirname, '..', '..', '..');
const sidecar = path.resolve(__dirname, '..');
const resources = path.join(repo, 'crates', 'desktop', 'artcraft', 'resources');

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  const resolved = path.resolve(value);
  if (!fs.existsSync(resolved)) throw new Error(`${name} does not exist: ${resolved}`);
  return resolved;
}

function requiredEnv(name) {
  const value = process.env[name];
  if (!value || value === 'unknown') throw new Error(`${name} is required and cannot be unknown`);
  return value;
}

function copy(source, destination) {
  fs.rmSync(destination, { recursive: true, force: true });
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.cpSync(source, destination, { recursive: true });
}

function files(root, current = root) {
  return fs.readdirSync(current, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(current, entry.name);
    return entry.isDirectory() ? files(root, full) : [full];
  });
}

function findChrome(root) {
  return files(root).find((file) => path.basename(file).toLowerCase() === 'chrome.exe');
}

function stageDonutExtension(extension) {
  const destination = path.join(resources, 'donut-runtime', 'bundled-extensions', 'chromex.zip');
  const supplied = process.env.FLOWORD_CHROMEX_ZIP;
  if (supplied) {
    const zip = required('FLOWORD_CHROMEX_ZIP');
    copy(zip, destination);
    return destination;
  }
  if (process.platform !== 'win32') throw new Error('FLOWORD_CHROMEX_ZIP is required outside Windows staging');
  fs.rmSync(destination, { force: true });
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  childProcess.execFileSync('powershell.exe', [
    '-NoProfile', '-NonInteractive', '-Command',
    `Compress-Archive -Path ${JSON.stringify(path.join(extension, '*'))} -DestinationPath ${JSON.stringify(destination)} -Force`,
  ], { stdio: 'inherit' });
  if (!fs.existsSync(destination)) throw new Error('failed to create donut-runtime/bundled-extensions/chromex.zip');
  return destination;
}

const nodeRuntime = required('FLOWORD_NODE_RUNTIME');
const chromium = required('FLOWORD_CHROMIUM_DIR');
const extension = required('FLOWORD_CHROMEX_EXTENSION');
const donutRuntime = required('FLOWORD_DONUT_RUNTIME_EXE');
const sourceCommits = {
  donutbrowser: requiredEnv('FLOWORD_DONUT_COMMIT'),
  chromex: requiredEnv('FLOWORD_CHROMEX_COMMIT'),
  artcraft: requiredEnv('FLOWORD_ARTCRAFT_COMMIT'),
};
if (path.basename(nodeRuntime).toLowerCase() !== 'node.exe') throw new Error('FLOWORD_NODE_RUNTIME must point to node.exe');
if (!findChrome(chromium)) throw new Error(`FLOWORD_CHROMIUM_DIR contains no chrome.exe: ${chromium}`);
if (path.basename(donutRuntime).toLowerCase() !== 'floword-donut-runtime.exe') throw new Error('FLOWORD_DONUT_RUNTIME_EXE must point to floword-donut-runtime.exe');
if (!fs.existsSync(path.join(extension, 'manifest.json'))) throw new Error('FLOWORD_CHROMEX_EXTENSION is missing manifest.json');
if (!fs.existsSync(path.join(sidecar, 'node_modules', 'express', 'package.json'))) throw new Error('sidecar express dependency is missing; run npm ci');
if (!fs.existsSync(path.join(sidecar, 'node_modules', 'playwright', 'package.json'))) throw new Error('sidecar playwright dependency is missing; run npm ci');

copy(nodeRuntime, path.join(resources, 'node', path.basename(nodeRuntime)));
copy(donutRuntime, path.join(resources, 'donut-runtime', 'floword-donut-runtime.exe'));
stageDonutExtension(extension);
copy(chromium, path.join(resources, 'playwright'));
copy(extension, path.join(resources, 'chromex-extension'));
copy(path.join(sidecar, 'src'), path.join(resources, 'playwright-sidecar', 'src'));
copy(path.join(sidecar, 'package.json'), path.join(resources, 'playwright-sidecar', 'package.json'));
copy(path.join(sidecar, 'package-lock.json'), path.join(resources, 'playwright-sidecar', 'package-lock.json'));
copy(path.join(sidecar, 'node_modules'), path.join(resources, 'playwright-sidecar', 'node_modules'));

const manifestPath = path.join(resources, 'runtime-manifest.sha256.json');
fs.rmSync(manifestPath, { force: true });
const manifest = {};
for (const file of files(resources).sort()) {
  const relative = path.relative(resources, file).replaceAll(path.sep, '/');
  if (relative === 'runtime-manifest.sha256.json') continue;
  manifest[relative] = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}
const requiredArtifacts = [
  'donut-runtime/floword-donut-runtime.exe',
  'donut-runtime/bundled-extensions/chromex.zip',
  'node/node.exe',
  'playwright-sidecar/src/server.js',
  'playwright-sidecar/package.json',
  'playwright-sidecar/node_modules/express/package.json',
  'playwright-sidecar/node_modules/playwright/package.json',
  'chromex-extension/manifest.json',
];
const chromeArtifact = Object.keys(manifest).find((file) => file.startsWith('playwright/') && file.toLowerCase().endsWith('/chrome.exe'));
if (!chromeArtifact) throw new Error('staged runtime is missing playwright/**/chrome.exe');
requiredArtifacts.push(chromeArtifact);
for (const artifact of requiredArtifacts) if (!manifest[artifact]) throw new Error(`required production artifact missing: ${artifact}`);
const temporaryManifest = `${manifestPath}.tmp-${process.pid}`;
fs.writeFileSync(temporaryManifest, `${JSON.stringify({ schemaVersion: 1, generatedAt: new Date().toISOString(), sourceCommits, requiredArtifacts, files: manifest }, null, 2)}\n`, { flag: 'w' });
fs.renameSync(temporaryManifest, manifestPath);
console.log(`Staged ${Object.keys(manifest).length} runtime files in ${resources}`);
