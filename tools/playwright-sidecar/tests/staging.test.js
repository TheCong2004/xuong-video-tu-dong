const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const { stageDonutExtension, validateChromexZip } = require('../scripts/stage-production.cjs');

const repo = path.resolve(__dirname, '..', '..', '..', '..');
const script = path.join(__dirname, '..', 'scripts', 'stage-production.cjs');
const nodeRuntime = process.env.FLOWORD_NODE_RUNTIME || process.execPath;
const donutRuntime = process.env.FLOWORD_DONUT_RUNTIME_EXE || path.join(repo, 'donutbrowser', 'src-tauri', 'target', 'release', 'floword-donut-runtime.exe');
const extension = process.env.FLOWORD_CHROMEX_EXTENSION || path.join(repo, 'chromex', 'packages', 'extension', 'build', 'chrome-mv3-prod');

test('production staging is resumable, lock-protected, and CDP-only', { skip: process.platform !== 'win32' }, () => {
  assert.ok(fs.existsSync(nodeRuntime), `missing node runtime: ${nodeRuntime}`);
  assert.ok(fs.existsSync(donutRuntime), `missing Donut runtime: ${donutRuntime}`);
  assert.ok(fs.existsSync(path.join(extension, 'manifest.json')), `missing extension: ${extension}`);
  const output = fs.mkdtempSync(path.join(os.tmpdir(), 'floword-stage-'));
  const env = { ...process.env, FLOWORD_STAGE_OUTPUT_ROOT: output, FLOWORD_NODE_RUNTIME: nodeRuntime, FLOWORD_DONUT_RUNTIME_EXE: donutRuntime, FLOWORD_CHROMEX_EXTENSION: extension, FLOWORD_DONUT_COMMIT: 'bc5006a687c745b50a8c66d3be00e7c2d743dda1', FLOWORD_CHROMEX_COMMIT: '6bd87bfbed49e486f565f77ffa08b947a233df1b', FLOWORD_ARTCRAFT_COMMIT: '8266477c5196f3e907aa10e5606140c5f1cf7bf1' };
  const run = () => spawnSync(process.execPath, [script], { env, encoding: 'utf8', timeout: 180000 });
  try {
    assert.equal(run().status, 0);
    assert.equal(run().status, 0);
    const manifestPath = path.join(output, 'runtime-manifest.sha256.json');
    const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
    assert.equal(Object.keys(manifest.files).some((name) => name.startsWith('playwright/')), false);
    assert.ok(manifest.files['playwright-sidecar/src/server.js']);
    assert.ok(manifest.files['donut-runtime/donut-proxy.exe']);
    assert.equal(fs.existsSync(path.join(output, '.runtime-staging.lock')), false);
    fs.writeFileSync(path.join(output, '.runtime-staging.lock'), JSON.stringify({ pid: process.pid }));
    assert.notEqual(run().status, 0);
    fs.writeFileSync(path.join(output, '.runtime-staging.lock'), JSON.stringify({ pid: 999999 }));
    assert.equal(run().status, 0);
  } finally { fs.rmSync(output, { recursive: true, force: true }); }
});

test('Chromex ZIP validator rejects corrupt archives', { skip: process.platform !== 'win32' }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'floword-zip-'));
  const zip = path.join(dir, 'corrupt.zip');
  fs.writeFileSync(zip, 'not a zip archive');
  assert.throws(() => validateChromexZip(zip, undefined));
  fs.rmSync(dir, { recursive: true, force: true });
});

test('Chromex ZIP validator rejects traversal entries', { skip: process.platform !== 'win32' }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'floword-zip-'));
  const zip = path.join(dir, 'traversal.zip');
  const command = [
    '$ErrorActionPreference = "Stop"',
    'Add-Type -AssemblyName System.IO.Compression; Add-Type -AssemblyName System.IO.Compression.FileSystem',
    ` $z=[System.IO.Compression.ZipFile]::Open(${JSON.stringify(zip)}, [System.IO.Compression.ZipArchiveMode]::Create)`,
    'try { $e=$z.CreateEntry("../escape.txt"); $w=New-Object IO.StreamWriter($e.Open()); $w.Write("x"); $w.Dispose() } finally { $z.Dispose() }',
  ].join('; ');
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', command], { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert.throws(() => validateChromexZip(zip, undefined));
  fs.rmSync(dir, { recursive: true, force: true });
});

test('failed replacement preserves the previous valid ZIP SHA256', { skip: process.platform !== 'win32' }, () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'floword-zip-'));
  const oldZip = process.env.FLOWORD_CHROMEX_ZIP;
  const outputRoot = path.join(dir, 'output');
  fs.mkdirSync(outputRoot);
  try {
    delete process.env.FLOWORD_CHROMEX_ZIP;
    stageDonutExtension(extension, outputRoot);
    const destination = path.join(outputRoot, 'donut-runtime', 'bundled-extensions', 'chromex.zip');
    const before = require('node:crypto').createHash('sha256').update(fs.readFileSync(destination)).digest('hex');
    const corrupt = path.join(dir, 'corrupt.zip');
    fs.writeFileSync(corrupt, 'not a zip archive');
    process.env.FLOWORD_CHROMEX_ZIP = corrupt;
    assert.throws(() => stageDonutExtension(extension, outputRoot));
    const after = require('node:crypto').createHash('sha256').update(fs.readFileSync(destination)).digest('hex');
    assert.equal(after, before);
  } finally {
    if (oldZip === undefined) delete process.env.FLOWORD_CHROMEX_ZIP;
    else process.env.FLOWORD_CHROMEX_ZIP = oldZip;
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
