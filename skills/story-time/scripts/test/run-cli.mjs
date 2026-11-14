import { spawn } from 'node:child_process';

export function runCli(script, args, { cwd, env, input, nodeArgs = [] } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [...nodeArgs, script, ...args], { cwd, env });
    // A child may close stdin before reading it; its exit code still reports the outcome.
    child.stdin.on('error', (error) => {
      if (error.code !== 'EPIPE') reject(error);
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    child.on('close', (code) => resolve({ code, stdout, stderr }));
    child.stdin.end(input ?? '');
  });
}
