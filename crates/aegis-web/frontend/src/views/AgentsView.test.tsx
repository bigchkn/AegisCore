import { configureStore } from '@reduxjs/toolkit';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Provider } from 'react-redux';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AgentsView } from './AgentsView';
import { agentsReducer, setAgents } from '../store/agentsSlice';
import { channelsReducer } from '../store/channelsSlice';
import { projectsReducer } from '../store/projectsSlice';
import { tasksReducer } from '../store/tasksSlice';
import { setActiveProject, uiReducer } from '../store/uiSlice';

describe('AgentsView', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it('shows the spawn composer when no agents are active', () => {
    const store = makeStore();
    store.dispatch(setActiveProject('project-1'));

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <Routes>
            <Route path="/projects/:projectId/agents" element={<AgentsView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    expect(screen.getByRole('heading', { name: 'Spawn agent' })).toBeTruthy();
    expect(screen.getByPlaceholderText('Describe the task for the new agent')).toBeTruthy();
  });

  it('submits the spawn command through the existing API thunk', async () => {
    const store = makeStore();
    store.dispatch(setActiveProject('project-1'));
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(JSON.stringify({ task_id: 'task-1' }), { status: 200 })),
    );

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <Routes>
            <Route path="/projects/:projectId/agents" element={<AgentsView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    fireEvent.change(screen.getByPlaceholderText('Describe the task for the new agent'), {
      target: { value: 'Investigate the queue' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Spawn Agent' }));

    await waitFor(() =>
      expect(fetch).toHaveBeenCalledWith(
        '/projects/project-1/commands',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ command: 'spawn', params: 'Investigate the queue' }),
        }),
      ),
    );
  });

  it('navigates into the pane view when an agent is attached', async () => {
    const store = makeStore();
    store.dispatch(setActiveProject('project-1'));
    store.dispatch(setAgents([makeAgent('agent-1', 'Alpha')]));

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <Routes>
            <Route path="/projects/:projectId/agents" element={<AgentsView />} />
            <Route path="/projects/:projectId/pane/:agentId?" element={<div>Pane route</div>} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Attach' }));

    await waitFor(() => expect(screen.getByText('Pane route')).toBeDefined());
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
  });
}

function makeAgent(agentId: string, name: string) {
  return {
    agent_id: agentId,
    name,
    kind: 'bastion',
    status: 'active',
    role: 'worker',
    parent_id: null,
    task_id: null,
    tmux_session: 'aegis',
    tmux_window: 0,
    tmux_pane: '%1',
    worktree_path: '/tmp',
    cli_provider: 'claude-code',
    fallback_cascade: [],
    sandbox_profile: '/tmp/sandbox',
    log_path: '/tmp/log',
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    terminated_at: null,
  } as any;
}
