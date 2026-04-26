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
  };
  milestones: Record<string, { name: string; path: string; status: string }>;
  backlog?: string;
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

export type ActiveView = 'agents' | 'pane' | 'logs' | 'tasks' | 'channels' | 'taskflow';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected';
