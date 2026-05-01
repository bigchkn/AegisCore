export type ProjectStatus = {
  active_agents?: number;
  pending_tasks?: number;
  last_attached_agent_id?: string;
};

export type ProjectRecord = {
  id: string;
  root_path: string;
  auto_start: boolean;
  last_seen: string;
  name?: string;
  last_attached_agent_id?: string;
};

export type TaskflowIndex = {
  project: {
    name: string;
    current_milestone: number;
    backlog?: string;
  };
  milestones: Record<string, { name: string; path: string; status: string }>;
};

export type TaskType = 'feature' | 'bug' | 'maintenance';

export type TaskflowMilestone = {
  id: number;
  name: string;
  status: string;
  lld: string | null;
  tasks: Array<{
    id: string;
    uid: string;
    task: string;
    status: string;
    task_type: TaskType;
    crate_name: string | null;
    notes: string | null;
    registry_task_id: string | null;
  }>;
};

export type DesignDocSummary = {
  path: string;
  name: string;
  kind: string;
  bytes: number;
  modified_at: string | null;
};

export type DesignDocContent = {
  path: string;
  name: string;
  kind: string;
  content: string;
  modified_at: string | null;
};

export type DesignRefinementDraft = {
  doc_type: 'HLD' | 'LLD';
  doc_path: string;
  doc_description: string;
  bastion_agent_id?: string | null;
  hld_ref?: string | null;
  task_id?: string | null;
  provider?: string | null;
  model?: string | null;
};

export type ActiveView = 'agents' | 'pane' | 'logs' | 'tasks' | 'channels' | 'taskflow' | 'designs' | 'clarifications';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected';

export type ClarificationStatus = 'open' | 'answered' | 'rejected' | 'expired';

export type ClarifierSource = 'human_cli' | 'human_tui' | 'telegram' | 'system';

export type ClarificationResponse = {
  request_id: string;
  answer: string;
  payload: any;
  answered_by: ClarifierSource;
  created_at: string;
};

export type ClarificationRequest = {
  request_id: string;
  agent_id: string;
  task_id: string | null;
  question: string;
  context: any;
  priority: number;
  status: ClarificationStatus;
  created_at: string;
  answered_at: string | null;
  delivered_at: string | null;
  delivery_error: string | null;
  response: ClarificationResponse | null;
};
