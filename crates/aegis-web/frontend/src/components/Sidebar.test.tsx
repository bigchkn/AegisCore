import { configureStore } from '@reduxjs/toolkit';
import { render, screen } from '@testing-library/react';
import { Provider } from 'react-redux';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { Sidebar } from './Sidebar';
import { agentsReducer } from '../store/agentsSlice';
import { channelsReducer } from '../store/channelsSlice';
import { projectsReducer } from '../store/projectsSlice';
import { tasksReducer } from '../store/tasksSlice';
import { setActiveProject, uiReducer } from '../store/uiSlice';

describe('Sidebar', () => {
  it('preserves the query agent when navigating to other views', () => {
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

    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={['/projects/project-1/logs/agent-1?agent=agent-1']}>
          <Sidebar />
        </MemoryRouter>
      </Provider>,
    );

    expect(screen.getByRole('link', { name: 'Channels' }).getAttribute('href')).toBe(
      '/projects/project-1/channels?agent=agent-1',
    );
    expect(screen.getByRole('link', { name: 'Agents' }).getAttribute('href')).toBe(
      '/projects/project-1/agents?agent=agent-1',
    );
  });
});
