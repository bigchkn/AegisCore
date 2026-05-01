import { renderHook, act } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { useProjectViewState } from './useProjectViewState';

describe('useProjectViewState', () => {
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

  it('loads and stores project-scoped values', () => {
    window.localStorage.setItem('aegis.web.viewState.project-1.designs.listCollapsed', JSON.stringify(true));

    const { result } = renderHook(() =>
      useProjectViewState('project-1', 'designs.listCollapsed', false, isBoolean),
    );

    expect(result.current[0]).toBe(true);

    act(() => {
      result.current[1](false);
    });

    expect(window.localStorage.getItem('aegis.web.viewState.project-1.designs.listCollapsed')).toBe('false');
  });

  it('falls back when stored values do not pass validation', () => {
    window.localStorage.setItem('aegis.web.viewState.project-1.taskflow.expandedMilestones', JSON.stringify([1]));

    const { result } = renderHook(() =>
      useProjectViewState<string[]>('project-1', 'taskflow.expandedMilestones', [], isStringArray),
    );

    expect(result.current[0]).toEqual([]);
  });
});

function isBoolean(value: unknown): value is boolean {
  return typeof value === 'boolean';
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === 'string');
}
