import { configureStore } from '@reduxjs/toolkit';
import { render, act } from '@testing-library/react';
import { Provider } from 'react-redux';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { App } from './App';
import { agentsReducer } from '../store/agentsSlice';
import { channelsReducer } from '../store/channelsSlice';
import { projectsReducer, setProjects } from '../store/projectsSlice';
import { tasksReducer } from '../store/tasksSlice';
import { setActiveProject, uiReducer } from '../store/uiSlice';

class MockWebSocket {
  static OPEN = 1;
  static CONNECTING = 0;
  readyState = MockWebSocket.OPEN;
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent<string>) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  close() { this.readyState = 3; }
  send = vi.fn();
}

const PROJECT = {
  id: 'project-1',
  root_path: '/tmp/project-1',
  auto_start: false,
  last_seen: new Date().toISOString(),
  last_attached_agent_id: undefined,
};

function makeStore() {
  return configureStore({
    reducer: {
      agents: agentsReducer,
      channels: channelsReducer,
      projects: projectsReducer,
      tasks: tasksReducer,
      ui: uiReducer,
    },
  });
}

function makeFetch() {
  return vi.fn(async (url: string) => {
    if (url === '/projects') return new Response(JSON.stringify([PROJECT]), { status: 200 });
    if (url.endsWith('/agents')) return new Response(JSON.stringify([]), { status: 200 });
    if (url.endsWith('/tasks')) return new Response(JSON.stringify([]), { status: 200 });
    if (url.endsWith('/channels')) return new Response(JSON.stringify([]), { status: 200 });
    if (url.endsWith('/status')) return new Response(JSON.stringify({ active_agents: 0 }), { status: 200 });
    return new Response(JSON.stringify({}), { status: 200 });
  });
}

describe('App background polling', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', MockWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('polls fetchProjectData every 5 seconds when a project is active', async () => {
    const fetchMock = makeFetch();
    vi.stubGlobal('fetch', fetchMock);

    const store = makeStore();
    store.dispatch(setProjects([PROJECT]));
    store.dispatch(setActiveProject('project-1'));

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <App />
        </MemoryRouter>
      </Provider>,
    );

    // Flush the initial render effects
    await act(async () => { vi.runOnlyPendingTimers(); });

    const callsAfterMount = fetchMock.mock.calls.length;
    expect(callsAfterMount).toBeGreaterThan(0);

    // Advance one poll interval
    await act(async () => { vi.advanceTimersByTime(5_000); });

    expect(fetchMock.mock.calls.length).toBeGreaterThan(callsAfterMount);
  });

  it('stops polling when the component unmounts', async () => {
    const fetchMock = makeFetch();
    vi.stubGlobal('fetch', fetchMock);

    const store = makeStore();
    store.dispatch(setProjects([PROJECT]));
    store.dispatch(setActiveProject('project-1'));

    const { unmount } = render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <App />
        </MemoryRouter>
      </Provider>,
    );

    await act(async () => { vi.runOnlyPendingTimers(); });

    unmount();
    const callsAtUnmount = fetchMock.mock.calls.length;

    // Advance past several poll intervals after unmount
    await act(async () => { vi.advanceTimersByTime(15_000); });

    expect(fetchMock.mock.calls.length).toBe(callsAtUnmount);
  });
});
