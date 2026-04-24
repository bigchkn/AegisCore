import type { AgentStatus } from '../types/AgentStatus';

export function StatusBadge({ status }: { status: AgentStatus }) {
  return <span className={`status-badge status-${status}`}>{status}</span>;
}
