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

  it('shows built-in templates in the spawn modal', async () => {
    const store = makeStore();
    store.dispatch(setActiveProject('project-1'));
    vi.stubGlobal('fetch', vi.fn(async () => templateListResponse()));

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <Routes>
            <Route path="/projects/:projectId/agents" element={<AgentsView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Spawn New Agent' }));

    expect(await screen.findByRole('heading', { name: 'Spawn Agent' })).toBeTruthy();
    expect(screen.getByLabelText('Type')).toBeTruthy();
    expect(screen.getByLabelText('Provider')).toBeTruthy();
    expect(await screen.findByLabelText('taskflow-implementer')).toBeTruthy();
    expect(screen.getByText('Implements one taskflow task.')).toBeTruthy();
  });

  it('submits the custom prompt spawn command through the existing API thunk', async () => {
    const store = makeStore();
    store.dispatch(setActiveProject('project-1'));
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(templateListResponse())
      .mockResolvedValueOnce(new Response(JSON.stringify({ task_id: 'task-1' }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <Routes>
            <Route path="/projects/:projectId/agents" element={<AgentsView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Spawn New Agent' }));
    fireEvent.click(await screen.findByRole('tab', { name: 'Custom Prompt' }));
    fireEvent.change(screen.getByPlaceholderText('Describe the task for the new agent'), {
      target: { value: 'Investigate the queue' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Spawn' }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        '/projects/project-1/commands',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ command: 'spawn', params: 'Investigate the queue' }),
        }),
      ),
    );
  });

  it('spawns a selected built-in template with rendered variables', async () => {
    const store = makeStore();
    store.dispatch(setActiveProject('project-1'));
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(templateListResponse())
      .mockResolvedValueOnce(new Response(JSON.stringify({ agent_id: 'agent-2' }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/agents']}>
          <Routes>
            <Route path="/projects/:projectId/agents" element={<AgentsView />} />
          </Routes>
        </MemoryRouter>
      </Provider>,
    );

    fireEvent.click(screen.getByRole('button', { name: 'Spawn New Agent' }));
    fireEvent.click(await screen.findByLabelText('taskflow-implementer'));
    fireEvent.change(screen.getByLabelText('Task Description'), {
      target: { value: 'Build the modal' },
    });
    fireEvent.change(screen.getByLabelText('Bastion Agent Id'), {
      target: { value: 'coordinator-1' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Spawn' }));

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        '/projects/project-1/commands',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            command: 'design.spawn_template',
            params: {
              name: 'taskflow-implementer',
              vars: {
                task_description: 'Build the modal',
                bastion_agent_id: 'coordinator-1',
              },
              model: null,
              provider: 'claude-code',
            },
          }),
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

function templateListResponse() {
  return new Response(
    JSON.stringify({
      providers: ['claude-code', 'codex', 'dirac', 'gemini-cli'],
      templates: [
        {
          name: 'taskflow-bastion',
          description: 'Coordinates taskflow.',
          kind: 'bastion',
          version: '2',
          tags: ['taskflow'],
          role: 'bastion',
          provider: 'claude-code',
          model: 'sonnet',
          required: ['project_root'],
          optional: [],
        },
        {
          name: 'taskflow-implementer',
          description: 'Implements one taskflow task.',
          kind: 'splinter',
          version: '1',
          tags: ['taskflow'],
          role: 'taskflow-implementer',
          provider: 'claude-code',
          model: 'sonnet',
          required: ['project_root', 'task_description', 'bastion_agent_id'],
          optional: ['task_id'],
        },
      ],
    }),
    { status: 200 },
  );
}
