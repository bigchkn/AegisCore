import { describe, expect, it, beforeEach } from 'vitest';

import { persistSidebarOpen, setActiveProject, setSelectedAgent, uiReducer } from './uiSlice';

describe('uiSlice', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
        removeItem: (key: string) => storage.delete(key),
      },
    });
  });

  it('clears selected agent when active project changes', () => {
    const selected = uiReducer(undefined, setSelectedAgent('agent-1'));
    const result = uiReducer(selected, setActiveProject('project-1'));

    expect(result.activeProjectId).toBe('project-1');
    expect(result.selectedAgentId).toBeNull();
  });

  it('initializes sidebar state from local storage', () => {
    window.localStorage.setItem('aegis.web.sidebarOpen', 'false');

    const result = uiReducer(undefined, { type: '@@INIT' });

    expect(result.sidebarOpen).toBe(false);
  });

  it('persists sidebar state to local storage', () => {
    persistSidebarOpen(false);

    expect(window.localStorage.getItem('aegis.web.sidebarOpen')).toBe('false');
  });
});
