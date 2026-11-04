import { CLOSING, requireRemote } from '../ports/tracker-gateway.mjs';

// In-memory tracker for tests and dry runs. failOn(op, story) simulates outages.
export class MemoryTrackerGateway {
  constructor({ failOn } = {}) {
    this.items = new Map();
    this.next = 0;
    this.failOn = failOn;
  }

  url(id) {
    return `https://tracker.example/${id}`;
  }

  async create(story) {
    if (this.failOn?.('create', story)) throw new Error(`tracker rejected ${story.id}`);
    const id = String(++this.next);
    this.items.set(id, { title: story.title, open: !(story.status in CLOSING) });
    return { remoteId: id, url: this.url(id) };
  }

  async update(story) {
    const id = requireRemote(story);
    if (this.failOn?.('update', story)) throw new Error(`tracker rejected ${story.id}`);
    const item = this.items.get(id);
    if (!item) throw new Error(`remote item ${id} not found`);
    Object.assign(item, { title: story.title, open: !(story.status in CLOSING) });
    return { remoteId: id, url: this.url(id) };
  }

  async remove(remoteId) {
    const item = this.items.get(String(remoteId));
    if (item) item.open = false;
  }
}
