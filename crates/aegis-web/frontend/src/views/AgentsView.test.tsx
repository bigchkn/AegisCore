import { configureStore } from '@reduxjs/toolkit';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Provider } from 'react-redux';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { AgentsView } from './AgentsView';
import { agentsReducer } from '../store/agentsSlice';
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
        <AgentsView />
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
        <AgentsView />
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
