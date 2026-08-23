#!/usr/bin/env node
// Stage the immutable runtime inputs used by the packaged Tauri app.
// Required environment variables:
//   FLOWORD_NODE_RUNTIME       path to node.exe
//   FLOWORD_CHROMIUM_DIR       Playwright Chromium directory
//   FLOWORD_CHROMEX_EXTENSION  unpacked extension directory

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');

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

const nodeRuntime = required('FLOWORD_NODE_RUNTIME');
const chromium = required('FLOWORD_CHROMIUM_DIR');
const extension = required('FLOWORD_CHROMEX_EXTENSION');
if (!findChrome(chromium)) throw new Error(`FLOWORD_CHROMIUM_DIR contains no chrome.exe: ${chromium}`);

copy(nodeRuntime, path.join(resources, 'node', path.basename(nodeRuntime)));
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
const temporaryManifest = `${manifestPath}.tmp-${process.pid}`;
fs.writeFileSync(temporaryManifest, `${JSON.stringify({ generatedAt: new Date().toISOString(), files: manifest }, null, 2)}\n`, { flag: 'w' });
fs.renameSync(temporaryManifest, manifestPath);
console.log(`Staged ${Object.keys(manifest).length} runtime files in ${resources}`);
