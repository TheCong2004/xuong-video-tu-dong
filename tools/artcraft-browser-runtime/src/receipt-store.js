'use strict';

const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');

function profileFile(root, profileId) {
  const digest = crypto.createHash('sha256').update(profileId).digest('hex').slice(0, 24);
  return path.join(root, `${digest}.receipt.json`);
}

class FileReceiptStore {
  constructor(root, fsImpl = fs) {
    this.root = root;
    this.fs = fsImpl;
  }

  async read(profileId) {
    try {
      const raw = await this.fs.readFile(profileFile(this.root, profileId), 'utf8');
      const value = JSON.parse(raw);
      if (!value || value.schemaVersion !== 1 || value.profileId !== profileId) return null;
      return value;
    } catch (error) {
      if (error && error.code === 'ENOENT') return null;
      throw new Error('RECEIPT_CORRUPT');
    }
  }

  async write(profileId, receipt) {
    await this.fs.mkdir(this.root, { recursive: true });
    const destination = profileFile(this.root, profileId);
    const temporary = `${destination}.${process.pid}.${Date.now()}.tmp`;
    await this.fs.writeFile(temporary, JSON.stringify(receipt), { encoding: 'utf8', mode: 0o600 });
    await this.fs.rename(temporary, destination);
    return receipt;
  }

  async clear(profileId) {
    try {
      await this.fs.unlink(profileFile(this.root, profileId));
    } catch (error) {
      if (!error || error.code !== 'ENOENT') throw error;
    }
  }
}

class MemoryReceiptStore {
  constructor(seed = {}) { this.values = new Map(Object.entries(seed)); }
  async read(profileId) { return this.values.get(profileId) || null; }
  async write(profileId, receipt) { this.values.set(profileId, JSON.parse(JSON.stringify(receipt))); return receipt; }
  async clear(profileId) { this.values.delete(profileId); }
}

module.exports = { FileReceiptStore, MemoryReceiptStore, profileFile };
