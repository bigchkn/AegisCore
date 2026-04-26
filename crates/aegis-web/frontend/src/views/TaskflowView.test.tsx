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
    M2: { name: 'Milestone 2', path: 'milestones/M2.toml', status: 'done' },
  },
  backlog: 'backlog.toml',
};

const mockMilestone1 = {
  id: 1,
  name: 'Milestone 1',
  status: 'in-progress',
  tasks: [
    { id: '1.1', task: 'Active Feature', status: 'pending', task_type: 'feature' },
    { id: '1.2', task: 'Active Bug', status: 'pending', task_type: 'bug' },
  ],
};

const mockMilestone2 = {
  id: 2,
  name: 'Milestone 2',
  status: 'done',
  tasks: [
    { id: '2.1', task: 'Completed Bug', status: 'done', task_type: 'bug' },
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

describe('TaskflowView Refactored Filters', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('separates Milestone Tree from Flat Bugs View', async () => {
    (api.taskflowStatus as any).mockResolvedValue(mockIndex);
    (api.taskflowMilestone as any).mockImplementation((_pid: string, id: string) => {
      if (id === 'M1') return Promise.resolve(mockMilestone1);
      if (id === 'M2') return Promise.resolve(mockMilestone2);
      if (id === 'backlog') return Promise.resolve(mockBacklog);
      return Promise.reject(new Error('Not found'));
    });

    renderWithStore();

    // 1. Default state: Milestones View + Active
    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    expect(screen.queryByText('Milestone 2')).toBeNull(); // Completed milestone hidden

    // 2. Switch to Bugs view
    fireEvent.click(screen.getByText('Bugs'));
    
    // Should see flat list of bugs (Active + Completed because default state was Active, but Bugs shows ALL loaded until Active is clicked?)
    // Actually, showAll defaults to false. So "Completed Bug" should be hidden.
    await waitFor(() => expect(screen.getByText('Active Bug')).toBeDefined());
    await waitFor(() => expect(screen.getByText('Backlog Bug')).toBeDefined());
    expect(screen.queryByText('Completed Bug')).toBeNull(); 
    
    // Milestones should NOT be headers anymore (they are context labels now)
    // In our implementation, context labels have class 'task-context-label'
    expect(screen.getByText('Milestone 1', { selector: '.task-context-label' })).toBeDefined();

    // 3. Switch to 'All' status
    fireEvent.click(screen.getByText('All'));
    await waitFor(() => expect(screen.getByText('Completed Bug')).toBeDefined());

    // 4. Switch back to Milestones view
    fireEvent.click(screen.getByText('Milestones'));
    await waitFor(() => expect(screen.getByText('Milestone 1', { selector: '.milestone-name' })).toBeDefined());
    await waitFor(() => expect(screen.getByText('Milestone 2', { selector: '.milestone-name' })).toBeDefined());
  });
});
