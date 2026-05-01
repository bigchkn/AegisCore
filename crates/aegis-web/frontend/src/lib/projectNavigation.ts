import { agentRoute } from './agentRoutes';
import type { ActiveView, ProjectRecord } from '../store/domain';

const projectViewStoragePrefix = 'aegis.web.projectView.';

const activeViews: ActiveView[] = [
  'agents',
  'pane',
  'logs',
  'tasks',
  'channels',
  'taskflow',
  'designs',
  'clarifications',
];

export function viewFromPathParts(pathParts: string[]): ActiveView | null {
  const raw = pathParts.includes('projects') ? pathParts[2] : pathParts[0] || 'agents';
  return isActiveView(raw) ? raw : null;
}

export function routeProjectIdFromPathParts(pathParts: string[]) {
  return pathParts[0] === 'projects' ? pathParts[1] ?? null : null;
}

export function loadProjectView(projectId: string): ActiveView | null {
  if (!projectId || typeof window === 'undefined') return null;
  try {
    const value = window.localStorage.getItem(`${projectViewStoragePrefix}${projectId}`);
    return value && isActiveView(value) ? value : null;
  } catch {
    return null;
  }
}

export function saveProjectView(projectId: string, view: ActiveView) {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(`${projectViewStoragePrefix}${projectId}`, view);
  } catch {}
}

export function projectRouteForView(
  project: Pick<ProjectRecord, 'id' | 'last_attached_agent_id'>,
  view: ActiveView | null,
) {
  const targetView = view ?? (project.last_attached_agent_id ? 'pane' : 'agents');
  const agentId =
    (targetView === 'pane' || targetView === 'logs') && project.last_attached_agent_id
      ? project.last_attached_agent_id
      : null;

  return agentRoute(project.id, targetView, agentId);
}

function isActiveView(value: string | undefined): value is ActiveView {
  return activeViews.includes(value as ActiveView);
}
