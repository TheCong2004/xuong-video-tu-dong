import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import os from 'os';

const omnirouteDir = path.resolve(process.cwd(), 'app/src/pages/OmniRoute');
const publicDir = path.resolve(process.cwd(), 'app/public/omniroute-ui');

console.log('Building OmniRoute UI statically...');
try {
  const buildEnv = {
    ...process.env,
    OMNIROUTE_STATIC_EXPORT: '1',
    OMNIROUTE_BASE_PATH: '/omniroute-ui',
    NODE_OPTIONS: '--max-old-space-size=8192'
  };
  
  if (process.platform === 'win32') {
    const tmpDir = path.join(os.tmpdir(), `next-build-isolated-${Date.now()}`);
    fs.mkdirSync(path.join(tmpDir, 'AppData', 'Roaming'), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, 'AppData', 'Local'), { recursive: true });
    buildEnv.HOME = tmpDir;
    buildEnv.USERPROFILE = tmpDir;
    buildEnv.APPDATA = path.join(tmpDir, 'AppData', 'Roaming');
    buildEnv.LOCALAPPDATA = path.join(tmpDir, 'AppData', 'Local');
  }

  execSync('npx next build', {
    cwd: omnirouteDir,
    env: buildEnv,
    stdio: 'inherit'
  });

  console.log('Copying static UI to ArtCraft public directory...');
  if (fs.existsSync(publicDir)) {
    fs.rmSync(publicDir, { recursive: true, force: true });
  }
  fs.cpSync(path.join(omnirouteDir, 'out'), publicDir, { recursive: true });
  
  console.log('OmniRoute UI successfully bundled into ArtCraft!');
} catch (error) {
  console.error('Failed to build OmniRoute UI', error);
  process.exit(1);
}
