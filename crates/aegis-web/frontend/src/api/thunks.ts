import { createAsyncThunk } from '@reduxjs/toolkit';

import type { CustomSpawnOptions } from './rest';
import { api } from './rest';

export const fetchProjects = createAsyncThunk('projects/fetchProjects', api.listProjects);

export const fetchAgents = createAsyncThunk('agents/fetchAgents', async (projectId: string) =>
  api.listAgents(projectId),
);

export const fetchTasks = createAsyncThunk('tasks/fetchTasks', async (projectId: string) =>
  api.listTasks(projectId),
);

export const fetchChannels = createAsyncThunk('channels/fetchChannels', async (projectId: string) =>
  api.listChannels(projectId),
);

export const fetchProjectStatus = createAsyncThunk(
  'projects/fetchProjectStatus',
  async (projectId: string) => api.projectStatus(projectId),
);

export const fetchProjectData = createAsyncThunk(
  'projects/fetchProjectData',
  async (projectId: string, { dispatch }) => {
    await Promise.all([
      dispatch(fetchAgents(projectId)).unwrap(),
      dispatch(fetchTasks(projectId)).unwrap(),
      dispatch(fetchChannels(projectId)).unwrap(),
      dispatch(fetchProjectStatus(projectId)).unwrap(),
    ]);
  },
);

export const pauseAgent = createAsyncThunk(
  'agents/pauseAgent',
  async ({ projectId, agentId }: { projectId: string; agentId: string }) =>
    api.pause(projectId, agentId),
);

export const resumeAgent = createAsyncThunk(
  'agents/resumeAgent',
  async ({ projectId, agentId }: { projectId: string; agentId: string }) =>
    api.resume(projectId, agentId),
);

export const killAgent = createAsyncThunk(
  'agents/killAgent',
  async ({ projectId, agentId }: { projectId: string; agentId: string }) =>
    api.kill(projectId, agentId),
);

export const failoverAgent = createAsyncThunk(
  'agents/failoverAgent',
  async ({ projectId, agentId }: { projectId: string; agentId: string }) =>
    api.failover(projectId, agentId),
);

export const fetchDesignTemplates = createAsyncThunk(
  'agents/fetchDesignTemplates',
  async (projectId: string) => api.listDesignTemplates(projectId),
);

export const spawnTask = createAsyncThunk(
  'tasks/spawnTask',
  async ({
    projectId,
    task,
    options,
  }: {
    projectId: string;
    task: string;
    options?: CustomSpawnOptions;
  }) => api.spawn(projectId, task, options),
);

export const spawnDesignTemplate = createAsyncThunk(
  'tasks/spawnDesignTemplate',
  async ({
    projectId,
    name,
    vars,
    model,
    provider,
  }: {
    projectId: string;
    name: string;
    vars: Record<string, string>;
    model?: string;
    provider?: string;
  }) => api.spawnDesignTemplate(projectId, name, vars, model, provider),
);
