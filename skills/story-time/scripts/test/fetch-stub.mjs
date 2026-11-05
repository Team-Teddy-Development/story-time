// Preloaded with `node --import` by cli-judge.test.mjs: replaces fetch so the
// real CLI exercises the Jev path without the network. FAKE_JEV_STATUS forces
// an HTTP status for every request.
import { okBody } from './fake-jev.mjs';

const forced = process.env.FAKE_JEV_STATUS ? Number(process.env.FAKE_JEV_STATUS) : undefined;

globalThis.fetch = async (url, init) => {
  if (!String(url).startsWith('https://api.typesafe.ai/')) throw new Error(`fetch-stub: unexpected URL ${url}`);
  const request = JSON.parse(init.body);
  const status = forced ?? 200;
  const body = status === 200 ? okBody(request) : { error_type: 'stub' };
  return { ok: status === 200, status, headers: { get: () => null }, text: async () => JSON.stringify(body) };
};
