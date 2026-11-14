import { spawn } from 'node:child_process';

export function runCli(script, args, { cwd, env, input, nodeArgs = [] } = {}) {
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [...nodeArgs, script, ...args], { cwd, env });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    child.on('close', (code) => resolve({ code, stdout, stderr }));
    child.stdin.end(input ?? '');
  });
}
