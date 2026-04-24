export type ProjectStatus = {
  name?: string;
  root_path?: string;
  agents?: number;
  active_agents?: number;
  queued_tasks?: number;
  active_tasks?: number;
};

export type ProjectRecord = {
  project_id: string;
  name: string;
  root_path: string;
  registered_at?: string;
  last_seen_at?: string | null;
  status?: ProjectStatus;
};

export type ActiveView = 'agents' | 'pane' | 'logs' | 'tasks' | 'channels' | 'taskflow';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected';
