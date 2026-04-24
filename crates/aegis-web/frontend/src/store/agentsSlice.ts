import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

import type { Agent } from '../types/Agent';
import type { AgentStatus } from '../types/AgentStatus';
import { fetchAgents } from '../api/thunks';

type AgentStatusPatch = {
  agent_id: string;
  status: AgentStatus;
};

export type AgentsState = {
  items: Agent[];
  loading: boolean;
};

const initialState: AgentsState = {
  items: [],
  loading: false,
};

const agentsSlice = createSlice({
  name: 'agents',
  initialState,
  reducers: {
    setAgents(state, action: PayloadAction<Agent[]>) {
      state.items = action.payload;
      state.loading = false;
    },
    setAgentsLoading(state, action: PayloadAction<boolean>) {
      state.loading = action.payload;
    },
    upsertAgent(state, action: PayloadAction<Agent>) {
      const index = state.items.findIndex((agent) => agent.agent_id === action.payload.agent_id);
      if (index === -1) {
        state.items.push(action.payload);
      } else {
        state.items[index] = action.payload;
      }
    },
    updateAgentStatus(state, action: PayloadAction<AgentStatusPatch>) {
      const agent = state.items.find((item) => item.agent_id === action.payload.agent_id);
      if (agent) {
        agent.status = action.payload.status;
      }
    },
    removeAgent(state, action: PayloadAction<string>) {
      state.items = state.items.filter((agent) => agent.agent_id !== action.payload);
    },
  },
  extraReducers: (builder) => {
    builder
      .addCase(fetchAgents.pending, (state) => {
        state.loading = true;
      })
      .addCase(fetchAgents.fulfilled, (state, action) => {
        state.items = action.payload;
        state.loading = false;
      })
      .addCase(fetchAgents.rejected, (state) => {
        state.loading = false;
      });
  },
});

export const { setAgents, setAgentsLoading, upsertAgent, updateAgentStatus, removeAgent } =
  agentsSlice.actions;
export const agentsReducer = agentsSlice.reducer;
