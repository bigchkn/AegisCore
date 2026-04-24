import { configureStore } from '@reduxjs/toolkit';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { agentsReducer } from './agentsSlice';
import { channelsReducer } from './channelsSlice';
import { projectsReducer } from './projectsSlice';
import { tasksReducer } from './tasksSlice';
import { uiReducer } from './uiSlice';
import { wsMiddleware } from './wsMiddleware';
import { makeAgent } from './agentsSlice.test';
import type { AegisEvent } from '../types/AegisEvent';

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  static OPEN = 1;
  static CONNECTING = 0;

  readyState = MockWebSocket.OPEN;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(readonly url: string) {
    MockWebSocket.instances.push(this);
  }

  close() {
    this.readyState = 3;
  }

  send = vi.fn();

  emit(event: AegisEvent) {
    this.onmessage?.({ data: JSON.stringify(event) } as MessageEvent<string>);
  }
}

describe('wsMiddleware', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('dispatches status changed events into the agents slice', () => {
    const store = makeStore();
    vi.runOnlyPendingTimers();
    store.dispatch({ type: 'agents/upsertAgent', payload: makeAgent({ agent_id: 'a1' }) });

    MockWebSocket.instances[0].emit({
      type: 'agent_status_changed',
      agent_id: 'a1',
      old_status: 'active',
      new_status: 'paused',
    });

    expect(store.getState().agents.items[0].status).toBe('paused');
  });

  it('surfaces failover events as an operator banner', () => {
    const store = makeStore();
    vi.runOnlyPendingTimers();

    MockWebSocket.instances[0].emit({
      type: 'failover_initiated',
      agent_id: 'a1',
      from_provider: 'codex',
      to_provider: 'claude-code',
    });

    expect(store.getState().ui.error).toContain('codex -> claude-code');
  });
});

function makeStore() {
  return configureStore({
    reducer: {
      agents: agentsReducer,
      channels: channelsReducer,
      projects: projectsReducer,
      tasks: tasksReducer,
      ui: uiReducer,
    },
    middleware: (getDefaultMiddleware) => getDefaultMiddleware().concat(wsMiddleware),
  });
}
