import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
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
    taskflowCreateTask: vi.fn(),
    taskflowUpdateTask: vi.fn(),
    taskflowCreateMilestone: vi.fn(),
  },
}));

const mockIndex = {
  project: { 
    name: 'Test Project', 
    current_milestone: 1,
    backlog: 'backlog.toml' 
  },
  milestones: {
    M1: { name: 'Milestone 1', path: 'milestones/M1.toml', status: 'in-progress' },
    M2: { name: 'Milestone 2', path: 'milestones/M2.toml', status: 'done' },
  },
};

const mockMilestone1 = {
  id: 1,
  name: 'Milestone 1',
  status: 'in-progress',
  tasks: [
    { id: '1.1', uid: '11111111-1111-1111-1111-111111111111', task: 'Active Feature', status: 'in-progress', task_type: 'feature', crate_name: null, notes: null, registry_task_id: null },
    { id: '1.2', uid: '22222222-2222-2222-2222-222222222222', task: 'Pending Bug', status: 'pending', task_type: 'bug', crate_name: null, notes: null, registry_task_id: null },
  ],
};

const mockMilestone2 = {
  id: 2,
  name: 'Milestone 2',
  status: 'done',
  tasks: [
    { id: '2.1', uid: '33333333-3333-3333-3333-333333333333', task: 'Completed Bug', status: 'done', task_type: 'bug', crate_name: null, notes: null, registry_task_id: null },
  ],
};

const mockBacklog = {
  id: 0,
  name: 'Global Backlog',
  status: 'n/a',
  tasks: [
    { id: 'B1', uid: '44444444-4444-4444-4444-444444444444', task: 'Backlog Bug', status: 'pending', task_type: 'bug', crate_name: null, notes: null, registry_task_id: null },
  ],
};

function renderWithStore() {
  const initialState: UIState = {
    activeProjectId: 'proj-1',
    activeView: 'taskflow',
    selectedAgentId: null,
    error: null,
    connectionState: 'connected',
    sidebarOpen: true,
  };

  const store = configureStore({
    reducer: {
      ui: uiReducer,
      projects: () => ({ items: [{ id: 'proj-1', root_path: '/tmp', auto_start: false, last_seen: new Date().toISOString() }], loading: false }),
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

function mockTaskflowData() {
  (api.taskflowStatus as any).mockResolvedValue(mockIndex);
  (api.taskflowMilestone as any).mockImplementation((_pid: string, id: string) => {
    if (id === 'M1') return Promise.resolve(mockMilestone1);
    if (id === 'M2') return Promise.resolve(mockMilestone2);
    if (id === 'backlog') return Promise.resolve(mockBacklog);
    return Promise.reject(new Error('Not found'));
  });
}

describe('TaskflowView Refactored Filters', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    vi.clearAllMocks();
    storage.clear();
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
      },
    });
  });

  it('separates Milestone Tree from Flat Bugs View', async () => {
    mockTaskflowData();

    renderWithStore();

    // 1. Default state: Milestones View + Active
    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    expect(screen.queryByText('Milestone 2')).toBeNull(); // Completed milestone hidden

    // 2. Switch to Bugs view
    fireEvent.click(screen.getByText('Bugs'));
    
    // Should see flat list of bugs (Active + Completed because default state was Active, but Bugs shows ALL loaded until Active is clicked?)
    // Actually, showAll defaults to false. So "Completed Bug" should be hidden.
    await waitFor(() => expect(screen.getByText('Pending Bug')).toBeDefined());
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

  it('opens a bug modal with backlog defaults', async () => {
    mockTaskflowData();
    renderWithStore();

    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    fireEvent.click(screen.getByText('New Bug'));

    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('New Task')).toBeDefined();
    expect((within(dialog).getByLabelText('Task ID') as HTMLInputElement).value).toBe('');
    expect((within(dialog).getByLabelText(/^Task$/) as HTMLInputElement).value).toBe('');
    expect((within(dialog).getByLabelText('Type') as HTMLSelectElement).value).toBe('bug');
    expect((within(dialog).getByLabelText('Target') as HTMLSelectElement).value).toBe('backlog');
  });

  it('restores taskflow filters and expanded milestones', async () => {
    window.localStorage.setItem('aegis.web.viewState.proj-1.taskflow.viewMode', JSON.stringify('bugs'));
    window.localStorage.setItem('aegis.web.viewState.proj-1.taskflow.showAll', JSON.stringify(true));
    window.localStorage.setItem('aegis.web.viewState.proj-1.taskflow.expandedMilestones', JSON.stringify(['M1']));
    mockTaskflowData();

    renderWithStore();

    await waitFor(() => expect(screen.getByText('Completed Bug')).toBeDefined());
    expect(screen.getByText('Bugs').className).toContain('is-active');
    expect(screen.getByText('All').className).toContain('is-active');

    fireEvent.click(screen.getByText('Milestones'));

    await waitFor(() => expect(screen.getByText('Active Feature')).toBeDefined());
    expect(api.taskflowMilestone).toHaveBeenCalledWith('proj-1', 'M1');
  });

  it('saves task edits and surfaces notify warnings', async () => {
    mockTaskflowData();
    (api.taskflowUpdateTask as any).mockResolvedValue({
      task: {
        id: '1.2',
        uid: '22222222-2222-2222-2222-222222222222',
        task: 'Pending Bug',
        status: 'pending',
        task_type: 'bug',
        crate_name: null,
        notes: 'updated notes',
        registry_task_id: null,
      },
      notified: 0,
      warning: 'No active bastion was available to notify.',
    });

    renderWithStore();

    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    fireEvent.click(screen.getByText('Milestone 1', { selector: '.milestone-name' }).closest('button')!);
    await waitFor(() => expect(screen.getByText('Active Feature')).toBeDefined());
    fireEvent.click(screen.getAllByText('Edit')[1]);

    fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'updated notes' } });
    fireEvent.click(screen.getByText('Save'));

    await waitFor(() =>
      expect(api.taskflowUpdateTask).toHaveBeenCalledWith(
        'proj-1',
        'M1',
        '22222222-2222-2222-2222-222222222222',
        expect.objectContaining({
          notes: 'updated notes',
          target_milestone_id: 'M1',
        }),
      ),
    );

    await waitFor(() =>
      expect(screen.getByText('No active bastion was available to notify.')).toBeDefined(),
    );
  });

  it('creates a new milestone', async () => {
    mockTaskflowData();
    (api.taskflowCreateMilestone as any).mockResolvedValue({ message: 'Milestone created' });

    renderWithStore();

    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    fireEvent.click(screen.getByText('New Milestone'));

    const dialog = screen.getByRole('dialog');
    fireEvent.change(within(dialog).getByLabelText('Milestone ID'), { target: { value: 'M35' } });
    fireEvent.change(within(dialog).getByLabelText('Name'), { target: { value: 'My New Milestone' } });
    fireEvent.click(within(dialog).getByText('Create'));

    await waitFor(() =>
      expect(api.taskflowCreateMilestone).toHaveBeenCalledWith(
        'proj-1',
        'M35',
        'My New Milestone',
        undefined,
      ),
    );
  });

  it('creates a new bug with no task id', async () => {
    mockTaskflowData();
    (api.taskflowCreateTask as any).mockResolvedValue({
      task: {
        id: 'B2',
        uid: '55555555-5555-5555-5555-555555555555',
        task: 'Regression in login flow',
        status: 'pending',
        task_type: 'bug',
        crate_name: null,
        notes: null,
        registry_task_id: null,
      },
      notified: 0,
      warning: null,
    });

    renderWithStore();

    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    fireEvent.click(screen.getByText('New Bug'));

    const dialog = screen.getByRole('dialog');
    fireEvent.change(within(dialog).getByLabelText(/^Task$/), {
      target: { value: 'Regression in login flow' },
    });
    fireEvent.click(within(dialog).getByText('Save'));

    await waitFor(() =>
      expect(api.taskflowCreateTask).toHaveBeenCalledWith(
        'proj-1',
        'backlog',
        expect.objectContaining({
          task: 'Regression in login flow',
          task_type: 'bug',
          target_milestone_id: 'backlog',
        }),
      ),
    );
  });

  it('applies left-side highlight to active tasks but not non-active tasks', async () => {
    mockTaskflowData();
    renderWithStore();

    await waitFor(() => expect(screen.getByText('Milestone 1')).toBeDefined());
    fireEvent.click(screen.getByText('Milestone 1', { selector: '.milestone-name' }).closest('button')!);
    await waitFor(() => expect(screen.getByText('Active Feature')).toBeDefined());

    const activeTask = screen.getByText('Active Feature').closest('.taskflow-task');
    const pendingTask = screen.getByText('Pending Bug').closest('.taskflow-task');

    expect(activeTask?.getAttribute('data-status')).toBe('in-progress');
    expect(pendingTask?.getAttribute('data-status')).toBe('pending');
  });
});
