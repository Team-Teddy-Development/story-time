// Tracker tokens (PATs). Pure helpers; the file store adapter does the I/O.
// Tokens reach gh/az only through environment variables, never argv; the
// Gitea token goes only in the REST Authorization header.

export const SECRET_SPECS = {
  github: { file: '.github.env', variable: 'GH_TOKEN' },
  ado: { file: '.ado.env', variable: 'AZURE_DEVOPS_EXT_PAT' },
  gitea: { file: '.gitea.env', variable: 'GITEA_TOKEN' },
  jev: { file: '.jev.env', variable: 'TYPESAFE_API_KEY' },
};

export const SECRET_LABELS = { github: 'GitHub', ado: 'Azure DevOps', gitea: 'Gitea', jev: 'TypeSafe (Jev)' };

export function secretSpec(tracker) {
  const spec = SECRET_SPECS[tracker];
  if (!spec) throw new Error(`There is no token support for "${tracker}". Use: ${Object.keys(SECRET_SPECS).join(', ')}.`);
  return spec;
}

export function parseEnvFile(text) {
  const out = {};
  for (const line of text.split(/\r?\n/)) {
    const m = /^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$/.exec(line);
    if (!m || line.trimStart().startsWith('#')) continue;
    out[m[1]] = m[2].replace(/^(['"])(.*)\1$/, '$2');
  }
  return out;
}

export function formatEnvFile(variable, token) {
  return `# story-time: tracker token. Never commit this file.\n${variable}=${token}\n`;
}

export function mask(token) {
  return token && token.length > 8 ? `…${token.slice(-4)}` : '…';
}

// Env to hand to the tracker CLI. An already-set variable (CI, a shell export)
// wins, and gh/az fall back to their own login (`gh auth login`, `az login`).
export function tokenEnv(tracker, { file, env = process.env }) {
  const { variable } = secretSpec(tracker);
  if (env[variable] || !file) return {};
  return { [variable]: file };
}
