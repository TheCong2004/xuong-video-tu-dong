'use strict';

const { spawn } = require('node:child_process');

class RealProcessAdapter {
  constructor({ executable, spawnImpl = spawn, killImpl = process.kill } = {}) {
    this.executable = executable;
    this.spawnImpl = spawnImpl;
    this.killImpl = killImpl;
  }

  async spawn(spec) {
    const child = this.spawnImpl(spec.executable, spec.arguments, {
      cwd: spec.workingDirectory,
      shell: false,
      windowsHide: true,
      stdio: 'ignore',
    });
    return {
      pid: child.pid,
      child,
      executable: spec.executable,
      arguments: [...spec.arguments],
    };
  }

  async inspect(receipt) {
    if (!receipt || !receipt.browserPid) return false;
    try { this.killImpl(receipt.browserPid, 0); return true; } catch { return false; }
  }

  async terminate(receipt) {
    if (receipt && receipt.child && typeof receipt.child.kill === 'function') {
      receipt.child.kill();
      return;
    }
    if (receipt && receipt.browserPid) {
      try { this.killImpl(receipt.browserPid); } catch (error) {
        if (!error || error.code !== 'ESRCH') throw error;
      }
    }
  }
}

module.exports = { RealProcessAdapter };
