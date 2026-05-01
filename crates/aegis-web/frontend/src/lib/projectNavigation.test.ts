import { beforeEach, describe, expect, it } from 'vitest';

import {
  loadProjectView,
  projectRouteForView,
  routeProjectIdFromPathParts,
  saveProjectView,
  viewFromPathParts,
} from './projectNavigation';

describe('projectNavigation', () => {
  const storage = new Map<string, string>();

  beforeEach(() => {
    storage.clear();
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
      },
    });
  });

  it('derives project and view from project routes', () => {
    const parts = ['projects', 'project-1', 'designs'];

    expect(routeProjectIdFromPathParts(parts)).toBe('project-1');
    expect(viewFromPathParts(parts)).toBe('designs');
  });

  it('persists valid views per project', () => {
    saveProjectView('project-1', 'taskflow');

    expect(loadProjectView('project-1')).toBe('taskflow');
  });

  it('ignores invalid stored views', () => {
    window.localStorage.setItem('aegis.web.projectView.project-1', 'missing');

    expect(loadProjectView('project-1')).toBeNull();
  });

  it('routes pane and logs to the last attached agent when available', () => {
    const project = { id: 'project-1', last_attached_agent_id: 'agent-1' };

    expect(projectRouteForView(project, 'logs')).toBe('/projects/project-1/logs/agent-1?agent=agent-1');
    expect(projectRouteForView(project, null)).toBe('/projects/project-1/pane/agent-1?agent=agent-1');
  });
});
