import { afterEach, describe, expect, it, vi } from 'vitest';

import { api } from './rest';
import { makeAgent } from '../store/agentsSlice.test';

describe('REST API client', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('deserializes agent lists', async () => {
    const agent = makeAgent({ agent_id: 'a1' });
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify([agent]), { status: 200 })),
    );

    await expect(api.listAgents('project-1')).resolves.toEqual([agent]);
    expect(fetch).toHaveBeenCalledWith('/projects/project-1/agents', expect.any(Object));
  });

  it('throws on non-2xx responses', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('not found', { status: 404 })));

    await expect(api.listAgents('missing')).rejects.toThrow('not found');
  });

  it('encodes design document paths', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ path: 'x', name: 'x', kind: 'LLD', content: '', modified_at: null }), { status: 200 })),
    );

    await api.readDesignDoc('project-1', '.aegis/designs/lld/web ui.md');

    expect(fetch).toHaveBeenCalledWith(
      '/projects/project-1/designs/read?path=.aegis%2Fdesigns%2Flld%2Fweb%20ui.md',
      expect.any(Object),
    );
  });
});
