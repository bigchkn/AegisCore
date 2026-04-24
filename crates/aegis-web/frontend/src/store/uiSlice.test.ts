import { describe, expect, it } from 'vitest';

import { setActiveProject, setSelectedAgent, uiReducer } from './uiSlice';

describe('uiSlice', () => {
  it('clears selected agent when active project changes', () => {
    const selected = uiReducer(undefined, setSelectedAgent('agent-1'));
    const result = uiReducer(selected, setActiveProject('project-1'));

    expect(result.activeProjectId).toBe('project-1');
    expect(result.selectedAgentId).toBeNull();
  });
});
