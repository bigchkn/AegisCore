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
    (api.readDesignDoc as any).mockImplementation((_projectId: string, path: string) => {
      if (path === '.aegis/designs/lld/web-ui.md') {
        return Promise.resolve({
          path,
          name: 'web-ui.md',
          kind: 'LLD',
          content: '# Web UI\n\nPersisted document',
          modified_at: null,
        });
      }
      return Promise.resolve({
        path: '.aegis/designs/hld/aegis.md',
        name: 'aegis.md',
        kind: 'HLD',
        content: '# Aegis\n\nDesign content',
        modified_at: null,
      });
    });
  });

  it('lists and reads design documents', async () => {
    renderWithStore();

    await waitFor(() => expect(screen.getByText('aegis.md')).toBeDefined());
    expect(screen.getByText('web-ui.md')).toBeDefined();
    await waitFor(() => expect(screen.getByText(/Design content/)).toBeDefined());
    expect(api.readDesignDoc).toHaveBeenCalledWith('proj-1', '.aegis/designs/hld/aegis.md');
  });

  it('collapses and restores the design document list', async () => {
    renderWithStore();

    await waitFor(() => expect(screen.getByText('aegis.md')).toBeDefined());
    const toggle = screen.getByRole('button', { name: 'Hide List' });

    fireEvent.click(toggle);

    expect(screen.queryByLabelText('Design documents')).toBeNull();
    expect(screen.getByRole('button', { name: 'Show List' }).getAttribute('aria-expanded')).toBe('false');
    expect(screen.getByText(/Design content/)).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: 'Show List' }));

    expect(screen.getByLabelText('Design documents')).toBeDefined();
    expect(screen.getByRole('button', { name: 'Hide List' }).getAttribute('aria-expanded')).toBe('true');
  });

  it('restores selected document and collapsed list state', async () => {
    window.localStorage.setItem(
      'aegis.web.viewState.proj-1.designs.selectedPath',
      JSON.stringify('.aegis/designs/lld/web-ui.md'),
    );
    window.localStorage.setItem('aegis.web.viewState.proj-1.designs.listCollapsed', JSON.stringify(true));

    renderWithStore();

    await waitFor(() => expect(screen.getByRole('button', { name: 'Show List' })).toBeDefined());
    await waitFor(() =>
      expect(api.readDesignDoc).toHaveBeenCalledWith('proj-1', '.aegis/designs/lld/web-ui.md'),
    );
    expect(screen.queryByLabelText('Design documents')).toBeNull();
    expect(screen.getByText(/Persisted document/)).toBeDefined();
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
