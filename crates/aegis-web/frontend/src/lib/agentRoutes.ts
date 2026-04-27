export type AgentRouteView = 'pane' | 'logs';

export function agentRoute(projectId: string | null, view: string, agentId?: string | null) {
  const base = projectId ? `/projects/${projectId}/${view}` : `/${view}`;
  if (!agentId) {
    return base;
  }

  const withPath = view === 'pane' || view === 'logs' ? `${base}/${agentId}` : base;
  return `${withPath}?agent=${encodeURIComponent(agentId)}`;
}

export function agentIdFromLocation(pathname: string, search: string) {
  const fromPath = agentIdFromPath(pathname);
  if (fromPath) {
    return fromPath;
  }

  const params = new URLSearchParams(search);
  return params.get('agent');
}

function agentIdFromPath(pathname: string) {
  const parts = pathname.split('/').filter(Boolean);
  if (parts.length >= 4 && parts[0] === 'projects' && (parts[2] === 'pane' || parts[2] === 'logs')) {
    return parts[3] ?? null;
  }

  if (parts.length >= 2 && (parts[0] === 'pane' || parts[0] === 'logs')) {
    return parts[1] ?? null;
  }

  return null;
}
