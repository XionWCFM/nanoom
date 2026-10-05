const os = require('node:os');
const fs = require('node:fs');
const crypto = require('node:crypto');

function fingerprint(profile) {
  const canonical = Object.fromEntries(Object.keys(profile).sort().map(key => [key, profile[key]]));
  return crypto.createHash('sha256').update(JSON.stringify(canonical)).digest('hex');
}

// Standard cgroup-v2 mount; v1 or unavailable hierarchy stays explicitly unknown.
function cgroupLimits(read) {
  const membership = read('/proc/self/cgroup')?.split('\n').find(line => line.startsWith('0::'));
  if (!membership) return { cpuQuotaMilli: null, memoryLimitMiB: null, containerLimits: 'unknown' };
  const components = membership.slice(3).split('/').filter(Boolean);
  // A namespaced mount can report paths outside its visible root.
  const paths = components.includes('..') ? [] : components;
  let cpuQuotaMilli = null;
  let memoryLimitMiB = null;
  let detected = false;
  for (let depth = paths.length; depth >= 0; depth--) {
    const directory = '/sys/fs/cgroup' + (depth ? '/' + paths.slice(0, depth).join('/') : '');
    const quota = read(directory + '/cpu.max');
    const memory = read(directory + '/memory.max');
    detected ||= quota !== null && memory !== null;
    const parts = /^(\d+|max)\s+(\d+)$/.exec(quota || '');
    if (parts && parts[1] !== 'max' && Number(parts[1]) > 0 && Number(parts[2]) > 0) {
      const value = Math.max(1, Math.floor(Number(parts[1]) * 1000 / Number(parts[2])));
      if (Number.isSafeInteger(value)) cpuQuotaMilli = Math.min(cpuQuotaMilli ?? value, value);
    }
    if (/^\d+$/.test(memory || '') && Number(memory) > 0) {
      const value = Math.max(1, Math.floor(Number(memory) / 1048576));
      if (Number.isSafeInteger(value)) memoryLimitMiB = Math.min(memoryLimitMiB ?? value, value);
    }
  }
  return { cpuQuotaMilli, memoryLimitMiB, containerLimits: detected ? 'cgroup-v2' : 'unknown' };
}

function collect() {
  const read = path => {
    try { return fs.readFileSync(path, 'utf8').trim(); } catch { return null; }
  };
  const install = JSON.parse(process.env.INSTALL_RESULT || '{}');
  const profile = {
    version: 1,
    os: os.platform(),
    arch: os.arch(),
    cpuModel: os.cpus()[0]?.model || 'unknown',
    availableCpus: os.availableParallelism(),
    memoryMiB: Math.floor(os.totalmem() / 1048576),
    ...cgroupLimits(read),
    image: [process.env.ImageOS, process.env.ImageVersion].filter(Boolean).join('/'),
    nodeVersion: process.version,
    packageManager: install.packageManager || 'unknown',
    packageManagerVersion: install.packageManagerVersion || 'unknown',
  };
  return { fingerprint: fingerprint(profile), profile };
}

if (require.main === module) process.stdout.write(JSON.stringify(collect()));
module.exports = { fingerprint, collect, cgroupLimits };
