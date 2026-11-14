import { spawn } from 'node:child_process';

const INSTALL = {
  gh: 'Install the GitHub CLI (https://cli.github.com), or use story export for a copyable card.',
  az: 'Install the Azure CLI with the azure-devops extension (az extension add --name azure-devops), or use story export for a copyable card.',
};

// Runs a tracker CLI with an argument array: no shell, so no quoting or
// injection. Extra env (tokens) is merged into the child's environment only.
export function processExec(cmd, args, { input, env = {} } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { env: { ...process.env, ...env }, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (d) => (stdout += d));
    child.stderr.on('data', (d) => (stderr += d));
    child.on('error', (e) =>
      reject(e.code === 'ENOENT' ? new Error(`${cmd} is not installed or not on PATH. ${INSTALL[cmd] ?? ''}`.trim()) : e),
    );
    child.on('close', (code) => (code === 0 ? resolve(stdout) : reject(new Error(stderr.trim() || `${cmd} exited with code ${code}`))));
    // A tracker CLI may exit without reading stdin; its exit code still reports the outcome.
    child.stdin.on('error', (e) => {
      if (e.code !== 'EPIPE') reject(e);
    });
    child.stdin.end(input ?? '');
  });
}
