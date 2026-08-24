#!/usr/bin/env node
const { spawn, spawnSync } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');
const http = require('http');
const os = require('os');
const path = require('path');

async function main() {
  if (process.platform === 'win32') return;
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nanoom-download-smoke-'));
  const version = require('./package.json').version;
  const tag = `v${version}`;
  const platform = process.platform === 'darwin' ? 'macos' : 'linux';
  const arch = process.arch === 'x64' ? 'x64' : 'arm64';
  const archiveName = `nanoom-${platform}-${arch}.tar.gz`;
  const release = path.join(root, 'release', tag);
  const wrapper = path.join(root, 'wrapper');
  fs.mkdirSync(release, { recursive: true });
  fs.mkdirSync(path.join(wrapper, 'bin'), { recursive: true });
  fs.writeFileSync(path.join(root, 'nanoom'), `#!/usr/bin/env sh\necho nanoom ${version}\n`, { mode: 0o755 });
  spawnSync('tar', ['-czf', path.join(release, archiveName), '-C', root, 'nanoom'], { stdio: 'inherit' });
  fs.copyFileSync(path.join(__dirname, 'bin/nanoom.js'), path.join(wrapper, 'bin/nanoom.js'));
  fs.writeFileSync(path.join(wrapper, 'package.json'), JSON.stringify({ version, repository: 'https://github.com/XionWCFM/nanoom' }));

  const serverScript = `
    const fs=require('fs'),http=require('http'),path=require('path');
    const root=process.argv[1];
    const server=http.createServer((req,res)=>{const file=path.join(root,req.url);fs.createReadStream(file).on('error',()=>{res.statusCode=404;res.end()}).pipe(res)});
    server.listen(0,'127.0.0.1',()=>console.log(server.address().port));
  `;
  const server = spawn(process.execPath, ['-e', serverScript, path.join(root, 'release')], { stdio: ['ignore', 'pipe', 'inherit'] });
  const port = await new Promise((resolve, reject) => {
    server.stdout.once('data', data => resolve(String(data).trim()));
    server.once('error', reject);
  });
  const env = { ...process.env, HOME: path.join(root, 'home'), NANOOM_VERSION: tag, NANOOM_RELEASE_BASE_URL: `http://127.0.0.1:${port}` };
  const args = [path.join(wrapper, 'bin/nanoom.js'), '--version'];
  fs.writeFileSync(path.join(release, `${archiveName}.sha256`), `${'0'.repeat(64)}  ${archiveName}\n`);
  const rejected = spawnSync(process.execPath, args, { env, encoding: 'utf8' });
  if (rejected.status === 0 || !rejected.stderr.includes('Checksum verification failed')) throw new Error('bad checksum was not rejected');

  const archive = fs.readFileSync(path.join(release, archiveName));
  const checksum = crypto.createHash('sha256').update(archive).digest('hex');
  fs.writeFileSync(path.join(release, `${archiveName}.sha256`), `${checksum}  ${archiveName}\n`);
  const accepted = spawnSync(process.execPath, args, { env, encoding: 'utf8' });
  server.kill();
  if (accepted.status !== 0 || accepted.stdout.trim() !== `nanoom ${version}`) throw new Error(accepted.stderr || 'verified download failed');
  fs.rmSync(root, { recursive: true, force: true });
  console.log('npm fallback checksum smoke test passed');
}

main().catch(error => { console.error(error); process.exit(1); });
