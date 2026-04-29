import type { Agent } from '../types/Agent';
import type { ChannelRecord } from '../types/ChannelRecord';
import type { Task } from '../types/Task';
import type { 
  ProjectRecord, 
  ProjectStatus, 
  TaskflowIndex, 
  TaskflowMilestone,
  ClarificationRequest,
  ClarifierSource
} from '../store/domain';

type CommandResponse = {
  status?: string;
  task_id?: string;
  agent_id?: string;
  role?: string;
  kind?: string;
};

export type DesignTemplate = {
  name: string;
  description: string;
  kind: 'bastion' | 'splinter';
  version: string;
  tags: string[];
  role: string;
  provider: string;
  model: string | null;
  required: string[];
  optional: string[];
};

type DesignTemplateListResponse = {
  templates: DesignTemplate[];
};

type TaskMutationResponse = {
  task: TaskflowMilestone['tasks'][number];
  notified: number;
  warning?: string | null;
};

type TaskDraftPayload = {
  id?: string;
  task: string;
  task_type: TaskflowMilestone['tasks'][number]['task_type'];
  status?: string;
  crate_name?: string | null;
  notes?: string | null;
};

type TaskPatchPayload = {
  id?: string;
  task?: string;
  task_type?: TaskflowMilestone['tasks'][number]['task_type'];
  status?: string;
  crate_name?: string | null;
  notes?: string | null;
  target_milestone_id?: string;
};

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      'content-type': 'application/json',
      ...init?.headers,
    },
  });

  if (!response.ok) {
    let errorMessage = `${response.status} ${response.statusText}`;
    try {
      const errorBody = await response.text();
      if (errorBody) {
        try {
          const errorJson = JSON.parse(errorBody);
          if (errorJson && typeof errorJson === 'object') {
            errorMessage = errorJson.error || errorJson.message || JSON.stringify(errorJson);
          }
        } catch {
          errorMessage = errorBody;
        }
      }
    } catch {}
    throw new Error(errorMessage);
  }

  return (await response.json()) as T;
}

export const api = {
  listProjects: () => request<ProjectRecord[]>('/projects'),
  projectStatus: (projectId: string) => request<ProjectStatus>(`/projects/${projectId}/status`),
  listAgents: (projectId: string) => request<Agent[]>(`/projects/${projectId}/agents`),
  listTasks: (projectId: string) => request<Task[]>(`/projects/${projectId}/tasks`),
  listChannels: (projectId: string) => request<ChannelRecord[]>(`/projects/${projectId}/channels`),
  taskflowStatus: (projectId: string) =>
    request<TaskflowIndex>(`/projects/${projectId}/taskflow/status`),
  taskflowMilestone: (projectId: string, milestoneId: string) =>
    request<TaskflowMilestone>(`/projects/${projectId}/taskflow/show/${milestoneId}`),
  command: <T = CommandResponse>(projectId: string, command: string, params: unknown = null) =>
    request<T>(`/projects/${projectId}/commands`, {
      method: 'POST',
      body: JSON.stringify({ command, params }),
    }),
  spawn: (projectId: string, task: string) => api.command(projectId, 'spawn', task),
  listDesignTemplates: (projectId: string) =>
    api.command<DesignTemplateListResponse>(projectId, 'design.list'),
  spawnDesignTemplate: (
    projectId: string,
    name: string,
    vars: Record<string, string>,
    model?: string,
  ) =>
    api.command(projectId, 'design.spawn_template', {
      name,
      vars,
      model: model || null,
    }),
  taskflowCreateTask: (projectId: string, milestoneId: string, draft: TaskDraftPayload) =>
    api.command<TaskMutationResponse>(projectId, 'taskflow.create_task', {
      milestone_id: milestoneId,
      draft,
    }),
  taskflowCreateMilestone: (projectId: string, id: string, name: string, lld?: string) =>
    api.command(projectId, 'taskflow.create_milestone', {
      id,
      name,
      lld: lld || null,
    }),
  taskflowUpdateTask: (
    projectId: string,
    sourceMilestoneId: string,
    taskUid: string,
    patch: TaskPatchPayload,
  ) =>
    api.command<TaskMutationResponse>(projectId, 'taskflow.update_task', {
      source_milestone_id: sourceMilestoneId,
      task_uid: taskUid,
      patch,
    }),
  pause: (projectId: string, agentId: string) =>
    api.command(projectId, 'pause', { agent_id: agentId }),
  resume: (projectId: string, agentId: string) =>
    api.command(projectId, 'resume', { agent_id: agentId }),
  kill: (projectId: string, agentId: string) =>
    api.command(projectId, 'kill', { agent_id: agentId }),
  failover: (projectId: string, agentId: string) =>
    api.command(projectId, 'failover', { agent_id: agentId }),

  clarifyList: (projectId: string) => 
    request<ClarificationRequest[]>(`/projects/${projectId}/clarify/list`),
  
  clarifyAnswer: (projectId: string, requestId: string, answer: string, payload: unknown = {}, answeredBy: ClarifierSource = 'system') => 
    request<void>(`/projects/${projectId}/clarify/answer`, {
      method: 'POST',
      body: JSON.stringify({
        request_id: requestId,
        answer,
        payload,
        answered_by: answeredBy
      })
    }),
};
