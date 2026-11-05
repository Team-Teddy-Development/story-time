// Egress gate for Jev. Stories leave the machine only to the vendor origin,
// and only when they carry nothing that looks like personal data or a secret.
// Findings name a path and a kind, never the matched value.

export const JEV_VENDOR_ORIGIN = 'https://api.typesafe.ai';

export function assertVendorEndpoint(endpoint) {
  let url;
  try {
    url = new URL(endpoint);
  } catch {
    throw new Error(`Refusing Jev endpoint "${endpoint}": not a URL. Only ${JEV_VENDOR_ORIGIN} is allowed.`);
  }
  if (url.origin !== JEV_VENDOR_ORIGIN) {
    throw new Error(`Refusing Jev endpoint "${url.origin}": only ${JEV_VENDOR_ORIGIN} is allowed (no proxies or resellers).`);
  }
  return url.href;
}

const SHAPES = [
  ['email', /[\w.+-]+@[\w-]+\.[\w.-]+/],
  ['phone', /(?:\+?1[\s.-]?)?\(?\d{3}\)?[\s.-]?\d{3}[\s.-]?\d{4}\b/],
  ['secret', /\bgh[pousr]_[A-Za-z0-9]{20,}|\bgithub_pat_[A-Za-z0-9_]{20,}|\bAKIA[0-9A-Z]{16}\b|\bsk-[A-Za-z0-9_-]{20,}|\bxox[abprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----/],
];

export function scanPayload(value, path = '') {
  const findings = [];
  const walk = (v, p) => {
    if (typeof v === 'string') {
      for (const [kind, re] of SHAPES) if (re.test(v)) findings.push({ kind, path: p });
    } else if (Array.isArray(v)) {
      v.forEach((x, i) => walk(x, `${p}[${i}]`));
    } else if (v && typeof v === 'object') {
      for (const [k, x] of Object.entries(v)) walk(x, p ? `${p}.${k}` : k);
    }
  };
  walk(value, path);
  return findings;
}
