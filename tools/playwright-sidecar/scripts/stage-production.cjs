#!/usr/bin/env node
// Stage the immutable runtime inputs used by the packaged Tauri app.
// Required environment variables:
//   FLOWORD_NODE_RUNTIME       path to node.exe
//   FLOWORD_CHROMEX_EXTENSION  unpacked extension directory
//   FLOWORD_DONUT_RUNTIME_EXE  floword-donut-runtime.exe
//   FLOWORD_DONUT_PROXY_EXE    optional donut-proxy.exe (defaults to runtime sibling)

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const childProcess = require('node:child_process');

const repo = path.resolve(__dirname, '..', '..', '..');
const sidecar = path.resolve(__dirname, '..');
const resources = path.resolve(process.env.FLOWORD_STAGE_OUTPUT_ROOT || path.join(repo, 'crates', 'desktop', 'artcraft', 'resources'));
const stagingLock = path.join(resources, '.runtime-staging.lock');
function acquireStagingLock() {
  fs.mkdirSync(resources, { recursive: true });
  for (;;) {
    try {
      const fd = fs.openSync(stagingLock, 'wx');
      fs.writeFileSync(fd, JSON.stringify({ pid: process.pid, startedAt: new Date().toISOString() }));
      fs.closeSync(fd);
      return;
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      let owner;
      try { owner = JSON.parse(fs.readFileSync(stagingLock, 'utf8')); } catch (_) { throw new Error(`runtime staging lock is invalid: ${stagingLock}`); }
      let alive = false;
      if (Number.isInteger(owner.pid) && owner.pid > 0) {
        try { process.kill(owner.pid, 0); alive = true; } catch (_) { alive = false; }
      }
      if (alive) throw new Error(`runtime staging is already locked by pid ${owner.pid}: ${stagingLock}`);
      fs.rmSync(stagingLock, { force: true });
    }
  }
}

function releaseStagingLock() { fs.rmSync(stagingLock, { force: true }); }

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

function stageDonutExtension(extension, resourceRoot = resources) {
  const destination = path.join(resourceRoot, 'donut-runtime', 'bundled-extensions', 'chromex.zip');
  const temporary = `${destination}.tmp-${process.pid}.zip`;
  const supplied = process.env.FLOWORD_CHROMEX_ZIP;
  const expected = JSON.parse(fs.readFileSync(path.join(extension, 'manifest.json'), 'utf8'));
  if (expected.manifest_version !== 3 || !expected.version || !expected.background?.service_worker) throw new Error('Chromex manifest must be MV3 with version and background.service_worker');
  try {
    fs.rmSync(temporary, { force: true });
    fs.mkdirSync(path.dirname(destination), { recursive: true });
    if (supplied) copy(required('FLOWORD_CHROMEX_ZIP'), temporary);
    else {
      if (process.platform !== 'win32') throw new Error('FLOWORD_CHROMEX_ZIP is required outside Windows staging');
      childProcess.execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `Compress-Archive -Path ${JSON.stringify(path.join(extension, '*'))} -DestinationPath ${JSON.stringify(temporary)} -Force`], { stdio: 'inherit' });
    }
    validateChromexZip(temporary, expected.version);
    const backup = `${destination}.previous-${process.pid}`;
    fs.rmSync(backup, { force: true });
    if (fs.existsSync(destination)) fs.renameSync(destination, backup);
    try { fs.renameSync(temporary, destination); } catch (error) { if (fs.existsSync(backup)) fs.renameSync(backup, destination); throw error; }
    fs.rmSync(backup, { force: true });
    return destination;
  } catch (error) {
    fs.rmSync(temporary, { force: true });
    throw error;
  }
}

function validateChromexZip(zipPath, expectedVersion) {
  if (process.platform !== 'win32') throw new Error('Chromex ZIP validation requires Windows staging');
  const script = [
    '$ErrorActionPreference = "Stop"',
    'Add-Type -AssemblyName System.IO.Compression.FileSystem',
    `$z = [IO.Compression.ZipFile]::OpenRead(${JSON.stringify(zipPath)})`,
    'try {',
    '  $names = @($z.Entries | ForEach-Object { $_.FullName.Replace("\\", "/") })',
    '  if ($names.Count -gt 4096) { throw "ZIP entry limit exceeded" }',
    '  $totalSize = [int64](($z.Entries | Measure-Object -Property Length -Sum).Sum); if ($totalSize -gt 268435456) { throw "ZIP uncompressed size limit exceeded" }',
    '  $normalized = @($z.Entries | ForEach-Object { $_.FullName.Replace("\\", "/") })',
    '  if (($normalized | Sort-Object -Unique).Count -ne $normalized.Count) { throw "ZIP contains duplicate paths" }',
    '  if (@($normalized | Where-Object { $_ -eq "manifest.json" }).Count -ne 1) { throw "ZIP must contain exactly one root manifest.json" }',
    '  foreach ($n in $normalized) { $parts = $n.Split("/"); if ($n.StartsWith("/") -or $n -match "^[A-Za-z]:/" -or ($parts -contains "..")) { throw "ZIP path traversal: $n" } }',
    '  $m = $z.Entries | Where-Object { $_.FullName.Replace("\\", "/") -eq "manifest.json" } | Select-Object -First 1',
    '  $r = New-Object IO.StreamReader($m.Open()); try { $j = $r.ReadToEnd() } finally { $r.Dispose() }',
    '  $manifest = $j | ConvertFrom-Json',
    '  if ([int]$manifest.manifest_version -ne 3) { throw "Chromex manifest_version must be 3" }',
    '  if ([string]::IsNullOrWhiteSpace([string]$manifest.version)) { throw "Chromex manifest version missing" }',
    `  if (${JSON.stringify(expectedVersion || '')} -and [string]$manifest.version -ne ${JSON.stringify(expectedVersion || '')}) { throw "Chromex ZIP version mismatch" }`,
    '  if ([string]::IsNullOrWhiteSpace([string]$manifest.background.service_worker)) { throw "Chromex background.service_worker missing" }',
    '} finally { $z.Dispose() }',
  ].join('; ');
  childProcess.execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { stdio: 'inherit' });
}

function main() {
acquireStagingLock();
const nodeRuntime = required('FLOWORD_NODE_RUNTIME');
const extension = required('FLOWORD_CHROMEX_EXTENSION');
const donutRuntime = required('FLOWORD_DONUT_RUNTIME_EXE');
const donutProxy = process.env.FLOWORD_DONUT_PROXY_EXE
  ? required('FLOWORD_DONUT_PROXY_EXE')
  : path.join(path.dirname(donutRuntime), 'donut-proxy.exe');
if (!fs.existsSync(donutProxy)) throw new Error(`Donut proxy sidecar is required: ${donutProxy}`);
const sourceCommits = {
  donutbrowser: requiredEnv('FLOWORD_DONUT_COMMIT'),
  chromex: requiredEnv('FLOWORD_CHROMEX_COMMIT'),
  artcraft: requiredEnv('FLOWORD_ARTCRAFT_COMMIT'),
};
fs.rmSync(path.join(resources, 'playwright'), { recursive: true, force: true });
if (path.basename(nodeRuntime).toLowerCase() !== 'node.exe') throw new Error('FLOWORD_NODE_RUNTIME must point to node.exe');
if (path.basename(donutRuntime).toLowerCase() !== 'floword-donut-runtime.exe') throw new Error('FLOWORD_DONUT_RUNTIME_EXE must point to floword-donut-runtime.exe');
if (path.basename(donutProxy).toLowerCase() !== 'donut-proxy.exe') throw new Error('FLOWORD_DONUT_PROXY_EXE must point to donut-proxy.exe');
if (!fs.existsSync(path.join(extension, 'manifest.json'))) throw new Error('FLOWORD_CHROMEX_EXTENSION is missing manifest.json');
if (!fs.existsSync(path.join(sidecar, 'node_modules', 'express', 'package.json'))) throw new Error('sidecar express dependency is missing; run npm ci');
if (!fs.existsSync(path.join(sidecar, 'node_modules', 'playwright', 'package.json'))) throw new Error('sidecar playwright dependency is missing; run npm ci');

copy(nodeRuntime, path.join(resources, 'node', path.basename(nodeRuntime)));
copy(donutRuntime, path.join(resources, 'donut-runtime', 'floword-donut-runtime.exe'));
copy(donutProxy, path.join(resources, 'donut-runtime', 'donut-proxy.exe'));
stageDonutExtension(extension);
copy(extension, path.join(resources, 'chromex-extension'));
copy(path.join(sidecar, 'src'), path.join(resources, 'playwright-sidecar', 'src'));
copy(path.join(sidecar, 'package.json'), path.join(resources, 'playwright-sidecar', 'package.json'));
copy(path.join(sidecar, 'package-lock.json'), path.join(resources, 'playwright-sidecar', 'package-lock.json'));
copy(path.join(sidecar, 'node_modules'), path.join(resources, 'playwright-sidecar', 'node_modules'));

const manifestPath = path.join(resources, 'runtime-manifest.sha256.json');
const manifest = {};
for (const file of files(resources).sort()) {
  const relative = path.relative(resources, file).replaceAll(path.sep, '/');
  if (relative === 'runtime-manifest.sha256.json' || relative === '.runtime-staging.lock' || relative.includes('.tmp-') || relative.includes('.previous-')) continue;
  manifest[relative] = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}
const requiredArtifacts = [
  'donut-runtime/floword-donut-runtime.exe',
  'donut-runtime/donut-proxy.exe',
  'donut-runtime/bundled-extensions/chromex.zip',
  'node/node.exe',
  'playwright-sidecar/src/server.js',
  'playwright-sidecar/package.json',
  'playwright-sidecar/node_modules/express/package.json',
  'playwright-sidecar/node_modules/playwright/package.json',
  'chromex-extension/manifest.json',
];
for (const artifact of requiredArtifacts) if (!manifest[artifact]) throw new Error(`required production artifact missing: ${artifact}`);
const temporaryManifest = `${manifestPath}.tmp-${process.pid}`;
fs.writeFileSync(temporaryManifest, `${JSON.stringify({ schemaVersion: 1, generatedAt: new Date().toISOString(), sourceCommits, requiredArtifacts, files: manifest }, null, 2)}\n`, { flag: 'w' });
if (process.env.FLOWORD_STAGE_TEST_FAIL_AT === 'before-manifest-replace') {
  fs.rmSync(temporaryManifest, { force: true });
  throw new Error('injected staging failure before manifest replace');
}
fs.renameSync(temporaryManifest, manifestPath);
console.log(`Staged ${Object.keys(manifest).length} runtime files in ${resources}`);
releaseStagingLock();
}

if (require.main === module) main();

module.exports = { stageDonutExtension, validateChromexZip };
