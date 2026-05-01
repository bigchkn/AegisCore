import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react';
import { Provider } from 'react-redux';
import { configureStore } from '@reduxjs/toolkit';

import { DesignsView } from './DesignsView';
import { uiReducer, type UIState } from '../store/uiSlice';
import { api } from '../api/rest';

vi.mock('../api/rest', () => ({
  api: {
    listDesignDocs: vi.fn(),
    readDesignDoc: vi.fn(),
    startDesignRefinement: vi.fn(),
  },
}));

function renderWithStore() {
  const initialState: UIState = {
    activeProjectId: 'proj-1',
    activeView: 'designs',
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
      <DesignsView />
    </Provider>,
  );
}

describe('DesignsView', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (api.listDesignDocs as any).mockResolvedValue([
      {
        path: '.aegis/designs/hld/aegis.md',
        name: 'aegis.md',
        kind: 'HLD',
        bytes: 100,
        modified_at: null,
      },
      {
        path: '.aegis/designs/lld/web-ui.md',
        name: 'web-ui.md',
        kind: 'LLD',
        bytes: 120,
        modified_at: null,
      },
    ]);
    (api.readDesignDoc as any).mockResolvedValue({
      path: '.aegis/designs/hld/aegis.md',
      name: 'aegis.md',
      kind: 'HLD',
      content: '# Aegis\n\nDesign content',
      modified_at: null,
    });
  });

  it('lists and reads design documents', async () => {
    renderWithStore();

    await waitFor(() => expect(screen.getByText('aegis.md')).toBeDefined());
    expect(screen.getByText('web-ui.md')).toBeDefined();
    await waitFor(() => expect(screen.getByText(/Design content/)).toBeDefined());
    expect(api.readDesignDoc).toHaveBeenCalledWith('proj-1', '.aegis/designs/hld/aegis.md');
  });

  it('starts a design refinement cycle', async () => {
    (api.startDesignRefinement as any).mockResolvedValue({
      agent_id: 'agent-1',
      role: 'taskflow-designer',
      kind: 'Splinter',
    });

    renderWithStore();

    await waitFor(() => expect(screen.getByText('aegis.md')).toBeDefined());
    fireEvent.click(screen.getByText('New Refinement'));

    const dialog = screen.getByRole('dialog');
    fireEvent.change(within(dialog).getByLabelText('Document Path'), {
      target: { value: '.aegis/designs/lld/web-ui.md' },
    });
    fireEvent.change(within(dialog).getByLabelText('Refinement Brief'), {
      target: { value: 'Refine the web UI flow for design review.' },
    });
    fireEvent.change(within(dialog).getByLabelText('Coordinator ID'), {
      target: { value: 'bastion-1' },
    });
    fireEvent.click(within(dialog).getByText('Start'));

    await waitFor(() =>
      expect(api.startDesignRefinement).toHaveBeenCalledWith(
        'proj-1',
        expect.objectContaining({
          doc_type: 'HLD',
          doc_path: '.aegis/designs/lld/web-ui.md',
          doc_description: 'Refine the web UI flow for design review.',
          bastion_agent_id: 'bastion-1',
        }),
      ),
    );
  });
});
