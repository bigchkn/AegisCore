import { configureStore } from '@reduxjs/toolkit';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Provider } from 'react-redux';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { describe, expect, it, vi } from 'vitest';

import { LogView } from './LogView';
import { agentsReducer, setAgents } from '../store/agentsSlice';
import { channelsReducer } from '../store/channelsSlice';
import { projectsReducer } from '../store/projectsSlice';
import { tasksReducer } from '../store/tasksSlice';
import { setActiveProject, uiReducer } from '../store/uiSlice';

describe('LogView', () => {
  it('shows an agent dropdown when no agent is selected', () => {
    const store = configureStore({
      reducer: {
        agents: agentsReducer,
        channels: channelsReducer,
        projects: projectsReducer,
        tasks: tasksReducer,
        ui: uiReducer,
      },
    });

    store.dispatch(setActiveProject('project-1'));
    store.dispatch(setAgents([makeAgent('agent-1', 'Alpha')]));

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/logs']}>
          <Routes>
            <Route path="/projects/:projectId/logs/:agentId?" element={<LogView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    expect(screen.getByLabelText('Agent')).toBeTruthy();
  });

  it('shows an agent dropdown when attached', async () => {
    const store = configureStore({
      reducer: {
        agents: agentsReducer,
        channels: channelsReducer,
        projects: projectsReducer,
        tasks: tasksReducer,
        ui: uiReducer,
      },
    });

    store.dispatch(setActiveProject('project-1'));
    store.dispatch(setAgents([makeAgent('agent-1', 'Alpha'), makeAgent('agent-2', 'Beta')]));

    vi.stubGlobal(
      'WebSocket',
      class {
        onopen: (() => void) | null = null;
        onclose: (() => void) | null = null;
        onerror: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        constructor() {
          setTimeout(() => this.onopen?.(), 0);
        }
        close() {
          this.onclose?.();
        }
      } as any,
    );

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/logs/agent-1']}>
          <Routes>
            <Route path="/projects/:projectId/logs/:agentId?" element={<LogView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    expect(screen.getByLabelText('Agent')).toBeTruthy();
  });

  it('selects an agent from the picker', async () => {
    const store = configureStore({
      reducer: {
        agents: agentsReducer,
        channels: channelsReducer,
        projects: projectsReducer,
        tasks: tasksReducer,
        ui: uiReducer,
      },
    });

    store.dispatch(setActiveProject('project-1'));
    store.dispatch(setAgents([makeAgent('agent-1', 'Alpha'), makeAgent('agent-2', 'Beta')]));

    vi.stubGlobal(
      'WebSocket',
      class {
        onopen: (() => void) | null = null;
        onclose: (() => void) | null = null;
        onerror: (() => void) | null = null;
        onmessage: ((event: { data: string }) => void) | null = null;
        constructor() {
          setTimeout(() => this.onopen?.(), 0);
        }
        close() {
          this.onclose?.();
        }
      } as any,
    );

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/logs']}>
          <Routes>
            <Route path="/projects/:projectId/logs/agent-2" element={<div>Agent 2 route</div>} />
            <Route path="/projects/:projectId/logs/:agentId?" element={<LogView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    fireEvent.change(screen.getByLabelText('Agent'), { target: { value: 'agent-2' } });

    await waitFor(() => expect(screen.getByText('Agent 2 route')).toBeDefined());
  });
});

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
