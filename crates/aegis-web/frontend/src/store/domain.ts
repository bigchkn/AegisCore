export type ProjectStatus = {
  active_agents?: number;
  pending_tasks?: number;
};

export type ProjectRecord = {
  id: string;
  root_path: string;
  auto_start: boolean;
  last_seen: string;
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
    task: string;
    status: string;
    task_type: TaskType;
    crate_name: string | null;
    notes: string | null;
    registry_task_id: string | null;
  }>;
};

export type ActiveView = 'agents' | 'pane' | 'logs' | 'tasks' | 'channels' | 'taskflow' | 'clarifications';

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
