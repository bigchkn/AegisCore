import { useEffect, useState } from 'react';

export function useProjectViewState<T>(
  projectId: string | null,
  key: string,
  fallback: T,
  isValid: (value: unknown) => value is T,
) {
  const [value, setValue] = useState<T>(() => loadValue(projectId, key, fallback, isValid));

  useEffect(() => {
    setValue(loadValue(projectId, key, fallback, isValid));
  }, [projectId, key]);

  useEffect(() => {
    if (!projectId || typeof window === 'undefined') return;
    try {
      window.localStorage.setItem(storageKey(projectId, key), JSON.stringify(value));
    } catch {}
  }, [projectId, key, value]);

  return [value, setValue] as const;
}

function loadValue<T>(
  projectId: string | null,
  key: string,
  fallback: T,
  isValid: (value: unknown) => value is T,
) {
  if (!projectId || typeof window === 'undefined') return fallback;
  try {
    const raw = window.localStorage.getItem(storageKey(projectId, key));
    if (!raw) return fallback;
    const parsed = JSON.parse(raw);
    return isValid(parsed) ? parsed : fallback;
  } catch {
    return fallback;
  }
}

function storageKey(projectId: string, key: string) {
  return `aegis.web.viewState.${projectId}.${key}`;
}
