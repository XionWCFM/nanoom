#!/usr/bin/env node
// Entry point: resolves the platform-specific nanoom binary and execs it.
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

function repositorySlug() {
  if (process.env.NANOOM_REPOSITORY) return process.env.NANOOM_REPOSITORY;
  const repository = require('../package.json').repository;
  const value = typeof repository === 'string' ? repository : repository && repository.url;
  const match = String(value || '').match(/github\.com[/:]([^/]+\/[^/.]+?)(?:\.git)?$/);
  if (!match) throw new Error('Unable to determine the nanoom GitHub repository');
  return match[1];
}

const PLATFORM_PACKAGES = {
  'linux-x64': '@nanoom/cli-linux-x64',
  'linux-arm64': '@nanoom/cli-linux-arm64',
  'darwin-x64': '@nanoom/cli-macos-x64',
  'darwin-arm64': '@nanoom/cli-macos-arm64',
  'win32-x64': '@nanoom/cli-windows-x64',
};

function platformInfo() {
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];
  if (!pkg) {
    console.error(`Unsupported platform: ${key}`);
    process.exit(1);
  }
  return { key, pkg, archive: process.platform === 'win32' ? `nanoom-windows-x64.zip` : `nanoom-${process.platform === 'darwin' ? 'macos' : 'linux'}-${process.arch === 'x64' ? 'x64' : 'arm64'}.tar.gz` };
}

function installedBinary(info) {
  try {
    const pkgDir = path.dirname(require.resolve(`${info.pkg}/package.json`));
    const exe = path.join(pkgDir, process.platform === 'win32' ? 'nanoom.exe' : 'nanoom');
    if (fs.existsSync(exe)) return exe;
  } catch {
    // Fall through to the release download.
  }
  return null;
}

const info = platformInfo();
let executable = installedBinary(info);
if (!executable) {
  console.error(`[nanoom] ${info.pkg} was not installed for ${info.key}. Reinstall without --omit=optional, or install a matching release archive from https://github.com/${repositorySlug()}/releases.`);
  process.exit(1);
}

const child = spawn(executable, process.argv.slice(2), { stdio: 'inherit' });
child.on('close', (code) => process.exit(code ?? 1));
