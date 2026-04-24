import { describe, expect, it } from 'vitest';

import { agentsReducer, removeAgent, upsertAgent } from './agentsSlice';
import type { Agent } from '../types/Agent';

describe('agentsSlice', () => {
  it('upserts agents by id', () => {
    const agent = makeAgent({ agent_id: 'a1', status: 'active' });
    const updated = makeAgent({ agent_id: 'a1', status: 'paused' });

    const withAgent = agentsReducer(undefined, upsertAgent(agent));
    const result = agentsReducer(withAgent, upsertAgent(updated));

    expect(result.items).toHaveLength(1);
    expect(result.items[0].status).toBe('paused');
  });

  it('removes agents by id', () => {
    const state = agentsReducer(undefined, upsertAgent(makeAgent({ agent_id: 'a1' })));
    const result = agentsReducer(state, removeAgent('a1'));

    expect(result.items).toEqual([]);
  });
});

export function makeAgent(overrides: Partial<Agent> = {}): Agent {
  return {
    agent_id: 'agent-id',
    name: 'architect',
    kind: 'bastion',
    status: 'active',
    role: 'architect',
    parent_id: null,
    task_id: null,
    tmux_session: 'aegis',
    tmux_window: 0,
    tmux_pane: '%0',
    worktree_path: '/tmp/project',
    cli_provider: 'codex',
    fallback_cascade: ['codex'],
    sandbox_profile: '/tmp/profile.sb',
    log_path: '/tmp/log',
    created_at: '2026-04-24T00:00:00Z',
    updated_at: '2026-04-24T00:00:00Z',
    terminated_at: null,
    ...overrides,
  };
}
