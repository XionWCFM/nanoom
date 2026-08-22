#!/usr/bin/env node
// Verifies that a usable platform binary was installed via optionalDependencies.
const { execFileSync } = require('child_process');
const path = require('path');

const PLATFORM_PACKAGES = {
  'linux-x64': '@nanoom/cli-linux-x64',
  'linux-arm64': '@nanoom/cli-linux-arm64',
  'darwin-x64': '@nanoom/cli-macos-x64',
  'darwin-arm64': '@nanoom/cli-macos-arm64',
  'win32-x64': '@nanoom/cli-windows-x64',
};

const key = `${process.platform}-${process.arch}`;
const pkg = PLATFORM_PACKAGES[key];
if (!pkg) {
  console.warn(`[nanoom] Unsupported platform ${key}; skipping postinstall.`);
  process.exit(0);
}

try {
  const pkgDir = path.dirname(require.resolve(`${pkg}/package.json`));
  const exe = path.join(pkgDir, process.platform === 'win32' ? 'nanoom.exe' : 'nanoom');
  execFileSync(exe, ['--version'], { stdio: 'ignore' });
  console.log('[nanoom] binary installed successfully.');
} catch {
  // optionalDependencies may be skipped (e.g. --no-optional); the bin wrapper
  // will fall back to downloading from GitHub Releases at first run.
  console.warn('[nanoom] platform binary not found; it will be fetched on first use.');
}
