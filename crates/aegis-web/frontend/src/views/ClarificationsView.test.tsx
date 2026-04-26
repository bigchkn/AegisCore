import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { Provider } from 'react-redux';
import { configureStore } from '@reduxjs/toolkit';
import { ClarificationsView } from './ClarificationsView';
import { uiReducer } from '../store/uiSlice';
import { api } from '../api/rest';

vi.mock('../api/rest', () => ({
  api: {
    clarifyList: vi.fn(),
  },
}));

function renderWithStore() {
  const store = configureStore({
    reducer: {
      ui: uiReducer,
      projects: () => ({ items: [{ id: 'proj-1' }] }),
    },
    preloadedState: {
      ui: { 
        activeProjectId: 'proj-1',
        activeView: 'clarifications',
        selectedAgentId: null,
        error: null,
        connectionState: 'connected'
      } as any,
    },
  });

  return render(
    <Provider store={store}>
      <ClarificationsView />
    </Provider>
  );
}

describe('ClarificationsView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders empty state when no clarifications', async () => {
    (api.clarifyList as any).mockResolvedValue([]);
    renderWithStore();
    await waitFor(() => expect(screen.getByText('No pending clarifications')).toBeDefined());
  });

  it('renders clarification cards', async () => {
    const mockRequest = {
      request_id: 'req-1',
      agent_id: 'agent-1',
      question: 'What is the color of the sky?',
      priority: 1,
      status: 'open',
      created_at: new Date().toISOString(),
    };
    (api.clarifyList as any).mockResolvedValue([mockRequest]);
    
    renderWithStore();
    await waitFor(() => expect(screen.getByText('What is the color of the sky?')).toBeDefined());
    expect(screen.getByText('Agent: agent-1')).toBeDefined();
  });
});
