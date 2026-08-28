import { chmodSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const target = process.argv[2];
if (!['linux-amd64', 'linux-arm64'].includes(target)) process.exit(2);
const cargo = spawnSync('cargo', ['check', '--locked', '--quiet'], { stdio: 'inherit' });
if (cargo.status !== 0) process.exit(cargo.status ?? 1);
for (const key of Object.keys(process.env)) {
  if (key.includes('TOKEN') || key.includes('SECRET')) process.exit(8);
}
for (const secret of [
  '/run/secrets/build_token',
  '/run/build-secrets/build-token',
  '/run/secrets/release_token',
  '/run/secrets/publish_token',
  '/run/secrets/git_token',
]) {
  try {
    readFileSync(secret);
    process.exit(9);
  } catch {}
}
const manifest = readFileSync('vm-tool.yaml', 'utf8');
const version = manifest.match(/^version: ([^\s]+)$/m)?.[1];
if (!version) process.exit(3);
const stage = `dist/stage-${target}`;
const binary = `${stage}/bin/release-tool`;
rmSync(stage, { recursive: true, force: true });
mkdirSync(`${stage}/bin`, { recursive: true });
writeFileSync(binary, `#!/bin/sh\nprintf '%s\\n' '${version}'\n`);
chmodSync(binary, 0o755);
const archive = `dist/release-tool-${target}.tar.gz`;
const result = spawnSync('tar', ['-czf', archive, '-C', stage, 'bin'], { stdio: 'inherit' });
process.exit(result.status ?? 1);
