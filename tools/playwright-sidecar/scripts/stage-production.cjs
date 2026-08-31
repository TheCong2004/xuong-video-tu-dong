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
const REQUIRED_ARTIFACTS = [
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
  const startedAt = Date.now();
  const sourceStat = fs.statSync(source);
  console.log(JSON.stringify({ stage: 'ARTIFACT_STARTED', artifactName: path.basename(source), relativeSource: source, relativeDestination: destination, bytesCopied: 0, totalBytes: sourceStat.isFile() ? sourceStat.size : null, fileCount: sourceStat.isFile() ? 1 : null, elapsedMs: 0 }));
  fs.rmSync(destination, { recursive: true, force: true });
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.cpSync(source, destination, { recursive: true });
  const copied = sourceStat.isFile() ? [destination] : files(destination);
  let bytesCopied = 0;
  for (const file of copied) bytesCopied += fs.statSync(file).size;
  console.log(JSON.stringify({ stage: 'ARTIFACT_COPIED', artifactName: path.basename(source), relativeSource: source, relativeDestination: destination, bytesCopied, totalBytes: bytesCopied, fileCount: copied.length, elapsedMs: Date.now() - startedAt }));
}

function files(root, current = root) {
  return fs.readdirSync(current, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(current, entry.name);
    // Junctions/symlinked directories are generated roots, not source trees.
    // Never recurse through them during an incremental plan.
    if (entry.isSymbolicLink()) return [full];
    return entry.isDirectory() ? files(root, full) : [full];
  });
}

function realRoot(root) {
  return fs.realpathSync(root);
}

function inventory(root) {
  if (!fs.existsSync(root)) return new Map();
  const resolved = realRoot(root);
  const result = new Map();
  for (const file of files(resolved)) {
    const stat = fs.lstatSync(file);
    if (!stat.isFile()) continue;
    result.set(path.relative(resolved, file).replaceAll(path.sep, '/'), {
      absolute: file,
      size: stat.size,
      sha256: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'),
    });
  }
  return result;
}

function driveFreeBytes(root) {
  try {
    const probe = fs.existsSync(root) ? root : path.parse(path.resolve(root)).root;
    const stat = fs.statfsSync(probe);
    return Number(stat.bavail) * Number(stat.bsize);
  } catch (_) {
    return null;
  }
}

function incrementalPlan({ extension, zip, activeRoot, debugRoot, zipDestination, manifestPath: targetManifest = path.join(resources, 'runtime-manifest.sha256.json') }) {
  const plans = [];
  const addTree = (sourceRoot, destinationRoot, label) => {
    const source = inventory(sourceRoot);
    const destination = inventory(destinationRoot);
    for (const [relative, item] of source) {
      const destinationItem = destination.get(relative);
      const destinationPath = path.join(destinationRoot, relative);
      if (!destinationItem) plans.push({ kind: 'ADD', label, source: item.absolute, destination: destinationPath, size: item.size, relative });
      else if (destinationItem.sha256 !== item.sha256) plans.push({ kind: 'REPLACE', label, source: item.absolute, destination: destinationPath, size: item.size, relative });
      else plans.push({ kind: 'UNCHANGED', label, source: item.absolute, destination: destinationPath, size: item.size, relative });
    }
    for (const [relative, item] of destination) {
      if (!source.has(relative)) plans.push({ kind: 'REMOVE_STALE', label, source: null, destination: item.absolute, size: item.size, relative });
    }
  };
  addTree(extension, activeRoot, 'active-unpacked-extension');
  addTree(extension, debugRoot, 'debug-runtime-extension');
  const zipSourceStat = fs.statSync(zip);
  const zipDestinationStat = fs.existsSync(zipDestination) ? fs.statSync(zipDestination) : null;
  const zipHash = crypto.createHash('sha256').update(fs.readFileSync(zip)).digest('hex');
  if (!zipDestinationStat) plans.push({ kind: 'ADD', label: 'chromex-zip', source: zip, destination: zipDestination, size: zipSourceStat.size, relative: path.basename(zipDestination) });
  else {
    const existingHash = crypto.createHash('sha256').update(fs.readFileSync(zipDestination)).digest('hex');
    plans.push(existingHash === zipHash
      ? { kind: 'UNCHANGED', label: 'chromex-zip', source: zip, destination: zipDestination, size: zipSourceStat.size, relative: path.basename(zipDestination) }
      : { kind: 'REPLACE', label: 'chromex-zip', source: zip, destination: zipDestination, size: zipSourceStat.size, relative: path.basename(zipDestination) });
  }
  // Manifest is generated only after the tree is verified. It is deliberately
  // planned as a bounded replacement; the old manifest is never deleted first.
  const manifestBytes = Buffer.byteLength(JSON.stringify({ schemaVersion: 1, generatedAt: new Date().toISOString(), sourceCommits: {}, requiredArtifacts: REQUIRED_ARTIFACTS, files: {} }, null, 2) + '\n');
  plans.push({ kind: 'REPLACE', label: 'runtime-manifest', source: null, destination: targetManifest, size: manifestBytes, relative: path.basename(targetManifest) });
  const changed = plans.filter((p) => p.kind !== 'UNCHANGED');
  const unchanged = plans.filter((p) => p.kind === 'UNCHANGED');
  const stale = plans.filter((p) => p.kind === 'REMOVE_STALE');
  const adds = plans.filter((p) => p.kind === 'ADD');
  const replaces = plans.filter((p) => p.kind === 'REPLACE');
  const largestChangedFile = changed.reduce((max, p) => Math.max(max, p.size || 0), 0);
  const bytesToAdd = adds.reduce((sum, p) => sum + (p.size || 0), 0);
  const bytesToReplace = replaces.reduce((sum, p) => sum + (p.size || 0), 0);
  const safetyMargin = Math.max(64 * 1024 * 1024, 2 * largestChangedFile);
  const temporaryBytesRequired = bytesToAdd + bytesToReplace + largestChangedFile + safetyMargin;
  const expectedFinalDelta = bytesToAdd + bytesToReplace - stale.reduce((sum, p) => sum + (p.size || 0), 0);
  return {
    plans,
    changedFileCount: changed.length,
    unchangedFileCount: unchanged.length,
    staleFileCount: stale.length,
    bytesToAdd,
    bytesToReplace,
    largestChangedFile,
    temporaryBytesRequired,
    expectedFinalDelta,
    headroom: { dFreeBytes: driveFreeBytes(resources), cFreeBytes: driveFreeBytes(process.env.FLOWORD_STAGE_TRANSACTION_ROOT || 'C:\\'), requiredOnD: temporaryBytesRequired },
    zipHash,
  };
}

function writeJournal(file, event) {
  fs.appendFileSync(file, `${JSON.stringify({ timestamp: new Date().toISOString(), ...event })}\n`);
}

function transactionalApply(plan, { transactionRoot, sourceCommits, manifestPath: targetManifest, resourceRoot = resources } = {}) {
  const transactionId = `runtime-${Date.now()}-${process.pid}`;
  const tx = path.resolve(transactionRoot || process.env.FLOWORD_STAGE_TRANSACTION_ROOT || 'C:\\FlowordStageTransaction', transactionId);
  const backupRoot = path.join(tx, 'backup');
  const journal = path.join(tx, 'journal.ndjson');
  fs.mkdirSync(backupRoot, { recursive: true });
  fs.writeFileSync(journal, '');
  const changed = plan.plans.filter((item) => item.kind !== 'UNCHANGED' && item.label !== 'runtime-manifest');
  const backups = [];
  try {
    let appliedCount = 0;
    for (const item of changed) {
      const destination = item.destination;
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      const backup = path.join(backupRoot, `${backups.length}.bak`);
      if (fs.existsSync(destination)) {
        fs.copyFileSync(destination, backup);
        const backupHash = crypto.createHash('sha256').update(fs.readFileSync(backup)).digest('hex');
        if (backupHash !== crypto.createHash('sha256').update(fs.readFileSync(destination)).digest('hex')) throw new Error(`backup verification failed: ${destination}`);
        backups.push({ destination, backup, existed: true });
      } else backups.push({ destination, backup: null, existed: false });
      writeJournal(journal, { stage: 'BACKUP_COMPLETED', destination, backup });
      if (item.kind === 'REMOVE_STALE') { fs.rmSync(destination, { force: true }); writeJournal(journal, { stage: 'REMOVE_STALE', destination }); continue; }
      const part = `${destination}.part-${process.pid}`;
      fs.rmSync(part, { force: true });
      fs.copyFileSync(item.source, part);
      const sourceHash = crypto.createHash('sha256').update(fs.readFileSync(item.source)).digest('hex');
      const partHash = crypto.createHash('sha256').update(fs.readFileSync(part)).digest('hex');
      if (sourceHash !== partHash) throw new Error(`part hash mismatch: ${destination}`);
      fs.renameSync(part, destination);
      writeJournal(journal, { stage: 'REPLACE_COMPLETED', destination, sha256: partHash });
      appliedCount += 1;
      if (process.env.FLOWORD_STAGE_TEST_FAIL_AFTER && appliedCount >= Number(process.env.FLOWORD_STAGE_TEST_FAIL_AFTER)) throw new Error('injected incremental staging failure');
    }
    const manifest = {};
    for (const relative of REQUIRED_ARTIFACTS) {
      const file = path.join(resourceRoot, relative);
      if (!fs.existsSync(file) || !fs.statSync(file).isFile()) throw new Error(`required production artifact missing: ${relative}`);
      manifest[relative] = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
    }
    const temporaryManifest = `${targetManifest}.part-${process.pid}`;
    fs.writeFileSync(temporaryManifest, `${JSON.stringify({ schemaVersion: 1, generatedAt: new Date().toISOString(), sourceCommits, requiredArtifacts: REQUIRED_ARTIFACTS, files: manifest }, null, 2)}\n`);
    const readback = JSON.parse(fs.readFileSync(temporaryManifest, 'utf8'));
    for (const relative of REQUIRED_ARTIFACTS) if (readback.files[relative] !== manifest[relative]) throw new Error(`manifest readback mismatch: ${relative}`);
    if (fs.existsSync(targetManifest)) {
      const backup = path.join(backupRoot, 'manifest.bak');
      fs.copyFileSync(targetManifest, backup);
      backups.push({ destination: targetManifest, backup, existed: true });
    } else backups.push({ destination: targetManifest, backup: null, existed: false });
    fs.renameSync(temporaryManifest, targetManifest);
    writeJournal(journal, { stage: 'MANIFEST_COMPLETED', destination: targetManifest, files: REQUIRED_ARTIFACTS.length });
    return { transactionId, transactionRoot: tx, journal };
  } catch (error) {
    for (const item of backups.reverse()) {
      if (item.existed) { fs.mkdirSync(path.dirname(item.destination), { recursive: true }); fs.copyFileSync(item.backup, item.destination); }
      else fs.rmSync(item.destination, { recursive: true, force: true });
    }
    writeJournal(journal, { stage: 'ROLLBACK_COMPLETED', error: error.message });
    throw error;
  }
}

function stagedFiles(root) {
  const roots = ['node', 'donut-runtime', 'playwright-sidecar', 'chromex-extension'];
  return roots.flatMap((relative) => {
    const absolute = path.join(root, relative);
    return fs.existsSync(absolute) ? files(root, absolute) : [];
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

function incrementalMain({ dryRun = false } = {}) {
  const extension = required('FLOWORD_CHROMEX_EXTENSION');
  const zip = required('FLOWORD_CHROMEX_ZIP');
  const activeRoot = path.resolve(process.env.FLOWORD_ACTIVE_EXTENSION_ROOT || path.join(process.env.LOCALAPPDATA || 'C:\\Users\\Default\\AppData\\Local', 'DonutBrowserDev', 'bundled-extensions', 'chromex'));
  const debugRoot = path.resolve(process.env.FLOWORD_DEBUG_EXTENSION_ROOT || path.join(resources, 'chromex-extension'));
  const zipDestination = path.join(resources, 'donut-runtime', 'bundled-extensions', 'chromex.zip');
  const manifestPath = path.join(resources, 'runtime-manifest.sha256.json');
  const plan = incrementalPlan({ extension, zip, activeRoot, debugRoot, zipDestination, manifestPath });
  console.log(JSON.stringify({ stage: 'INCREMENTAL_PLAN', transactionRoot: process.env.FLOWORD_STAGE_TRANSACTION_ROOT || 'C:\\FlowordStageTransaction', ...plan, changed: plan.plans.filter((item) => item.kind !== 'UNCHANGED').map(({ kind, label, relative, size }) => ({ kind, label, relative, size })) }, null, 2));
  if (dryRun) return plan;
  const dFree = plan.headroom.dFreeBytes;
  if (dFree != null && dFree <= plan.temporaryBytesRequired) {
    throw new Error(`insufficient D: required ${plan.temporaryBytesRequired} bytes, free ${dFree} bytes, shortfall ${plan.temporaryBytesRequired - dFree} bytes`);
  }
  const sourceCommits = {
    donutbrowser: requiredEnv('FLOWORD_DONUT_COMMIT'),
    chromex: requiredEnv('FLOWORD_CHROMEX_COMMIT'),
    artcraft: requiredEnv('FLOWORD_ARTCRAFT_COMMIT'),
  };
  acquireStagingLock();
  try {
    return transactionalApply(plan, { transactionRoot: process.env.FLOWORD_STAGE_TRANSACTION_ROOT, sourceCommits, manifestPath, resourceRoot: resources });
  } finally {
    releaseStagingLock();
  }
}

function main() {
const args = new Set(process.argv.slice(2));
if (args.has('--incremental')) return incrementalMain({ dryRun: args.has('--dry-run') });
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
console.log(JSON.stringify({ stage: 'MANIFEST_STARTED', artifactName: 'runtime-manifest.sha256.json', relativeSource: null, relativeDestination: 'runtime-manifest.sha256.json', bytesCopied: 0, totalBytes: null, fileCount: null, elapsedMs: 0 }));
const manifest = {};
for (const relative of REQUIRED_ARTIFACTS) {
  const file = path.join(resources, relative);
  if (!fs.existsSync(file) || !fs.statSync(file).isFile()) throw new Error(`required production artifact missing: ${relative}`);
  manifest[relative] = crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}
for (const artifact of REQUIRED_ARTIFACTS) if (!manifest[artifact]) throw new Error(`required production artifact missing: ${artifact}`);
const temporaryManifest = `${manifestPath}.tmp-${process.pid}`;
fs.writeFileSync(temporaryManifest, `${JSON.stringify({ schemaVersion: 1, generatedAt: new Date().toISOString(), sourceCommits, requiredArtifacts: REQUIRED_ARTIFACTS, files: manifest }, null, 2)}\n`, { flag: 'w' });
if (process.env.FLOWORD_STAGE_TEST_FAIL_AT === 'before-manifest-replace') {
  fs.rmSync(temporaryManifest, { force: true });
  throw new Error('injected staging failure before manifest replace');
}
fs.renameSync(temporaryManifest, manifestPath);
console.log(JSON.stringify({ stage: 'MANIFEST_COMPLETED', artifactName: 'runtime-manifest.sha256.json', relativeSource: null, relativeDestination: 'runtime-manifest.sha256.json', bytesCopied: 0, totalBytes: null, fileCount: Object.keys(manifest).length, elapsedMs: 0 }));
console.log(`Staged ${Object.keys(manifest).length} runtime files in ${resources}`);
releaseStagingLock();
}

if (require.main === module) main();

module.exports = { stageDonutExtension, validateChromexZip, incrementalPlan, transactionalApply, REQUIRED_ARTIFACTS, inventory };
