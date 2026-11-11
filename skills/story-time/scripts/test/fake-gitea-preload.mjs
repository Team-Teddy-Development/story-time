// Preloaded with `node --import` by cli-push.test.mjs: replaces fetch with the
// Gitea simulator so the real CLI pushes without the network. State lives in
// the JSON file named by FAKE_GITEA_STATE so the test can inspect it.
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { fakeGiteaFetch, newGiteaState } from './fake-gitea.mjs';

const file = process.env.FAKE_GITEA_STATE;
const options = {
  baseUrl: process.env.FAKE_GITEA_BASE ?? 'https://gitea.example.com',
  owner: 'acme',
  repo: 'shop',
  token: process.env.FAKE_GITEA_TOKEN ?? 'gt_token',
};

globalThis.fetch = async (url, init) => {
  const state = file && existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : newGiteaState();
  const res = await fakeGiteaFetch(state, options)(url, init);
  if (file) writeFileSync(file, JSON.stringify(state));
  return res;
};
