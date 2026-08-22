#!/usr/bin/env node
// Local distribution smoke test. It emulates npm placing the current-platform
// optional package in node_modules, then executes the real wrapper.
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync } = require('child_process');

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nanoom-npm-smoke-'));
const platformName = process.platform === 'darwin' ? 'macos' : process.platform === 'win32' ? 'windows' : 'linux';
const archName = process.arch === 'x64' ? 'x64' : 'arm64';
const packageName = `@nanoom/cli-${platformName}-${archName}`;
const packageRoot = path.join(root, 'node_modules', packageName);
fs.mkdirSync(packageRoot, { recursive: true });
fs.writeFileSync(path.join(packageRoot, 'package.json'), JSON.stringify({ name: packageName }));
fs.copyFileSync(path.join(__dirname, '../../target/debug/nanoom'), path.join(packageRoot, 'nanoom'));
fs.chmodSync(path.join(packageRoot, 'nanoom'), 0o755);

const wrapperRoot = path.join(root, 'node_modules', '@nanoom', 'cli');
fs.mkdirSync(path.join(wrapperRoot, 'bin'), { recursive: true });
fs.copyFileSync(path.join(__dirname, 'bin/nanoom.js'), path.join(wrapperRoot, 'bin/nanoom.js'));
fs.copyFileSync(path.join(__dirname, 'package.json'), path.join(wrapperRoot, 'package.json'));

const output = execFileSync(process.execPath, [path.join(wrapperRoot, 'bin/nanoom.js'), '--version'], {
  encoding: 'utf8',
  cwd: root,
});
const expectedVersion = JSON.parse(fs.readFileSync(path.join(wrapperRoot, 'package.json'), 'utf8')).version;
if (output.trim() !== `nanoom ${expectedVersion}`) throw new Error(`unexpected version output: ${output}`);
fs.rmSync(root, { recursive: true, force: true });
console.log('npm wrapper smoke test passed');
