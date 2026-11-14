import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const created = new Set();

process.once('exit', () => {
  for (const dir of created) rmSync(dir, { recursive: true, force: true });
});

export async function makeTempDir(prefix) {
  const dir = mkdtempSync(join(tmpdir(), prefix));
  created.add(dir);
  return dir;
}
