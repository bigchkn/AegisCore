import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { Provider } from 'react-redux';
import { configureStore } from '@reduxjs/toolkit';
import { TaskflowView } from './TaskflowView';
import { uiReducer, type UIState } from '../store/uiSlice';
import { api } from '../api/rest';

// Mock the API
vi.mock('../api/rest', () => ({
  api: {
    taskflowStatus: vi.fn(),
    taskflowMilestone: vi.fn(),
  },
}));

const mockIndex = {
  project: { name: 'Test Project', current_milestone: 1 },
  milestones: {
    M1: { name: 'Milestone 1', path: 'milestones/M1.toml', status: 'in-progress' },
  },
  backlog: 'backlog.toml',
};

const mockMilestone = {
  id: 1,
  name: 'Milestone 1',
  status: 'in-progress',
  tasks: [
    { id: '1.1', task: 'A Feature', status: 'pending', task_type: 'feature' },
    { id: '1.2', task: 'A Bug', status: 'pending', task_type: 'bug' },
  ],
};

const mockBacklog = {
  id: 0,
  name: 'Global Backlog',
  status: 'n/a',
  tasks: [
    { id: 'B1', task: 'Backlog Bug', status: 'pending', task_type: 'bug' },
  ],
};

function renderWithStore() {
  const initialState: UIState = {
    activeProjectId: 'proj-1',
    activeView: 'taskflow',
    selectedAgentId: null,
    error: null,
    connectionState: 'connected',
  };

  const store = configureStore({
    reducer: {
      ui: uiReducer,
      projects: () => ({ items: [{ id: 'proj-1', root_path: '/tmp' }], loading: false }),
    },
    preloadedState: {
      ui: initialState,
    },
  });

  return render(
    <Provider store={store}>
      <TaskflowView />
    </Provider>
  );
}

describe('TaskflowView Bugs Filter', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('verifies bugs are visible when Bugs filter is active', async () => {
    (api.taskflowStatus as any).mockResolvedValue(mockIndex);
    (api.taskflowMilestone as any).mockImplementation((_pid: string, id: string) => {
      if (id === 'M1') return Promise.resolve(mockMilestone);
      if (id === 'backlog') return Promise.resolve(mockBacklog);
      return Promise.reject(new Error('Not found'));
    });

    renderWithStore();

    // Wait for index to load
    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());

    // Switch to Bugs filter
    const bugsBtn = screen.getByText('Bugs');
    fireEvent.click(bugsBtn);

    // FIX VERIFICATION: Bugs should now be visible because the filter effect auto-loads and expands them
    await waitFor(() => expect(screen.getByText('A Bug')).toBeDefined());
    await waitFor(() => expect(screen.getByText('Backlog Bug')).toBeDefined());
    
    // Feature should NOT be visible in Bugs filter
    expect(screen.queryByText('A Feature')).toBeNull();
  });
});
