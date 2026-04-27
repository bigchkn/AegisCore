export type AgentRouteView = 'pane' | 'logs';

export function agentRoute(projectId: string | null, view: AgentRouteView, agentId?: string | null) {
  const base = projectId ? `/projects/${projectId}/${view}` : `/${view}`;
  return agentId ? `${base}/${agentId}` : base;
}
