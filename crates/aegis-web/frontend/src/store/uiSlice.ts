import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

import type { ActiveView, ConnectionState } from './domain';

export type UIState = {
  activeProjectId: string | null;
  activeView: ActiveView;
  selectedAgentId: string | null;
  error: string | null;
  connectionState: ConnectionState;
  sidebarOpen: boolean;
};

const initialState: UIState = {
  activeProjectId: null,
  activeView: 'agents',
  selectedAgentId: null,
  error: null,
  connectionState: 'disconnected',
  sidebarOpen: true,
};

const uiSlice = createSlice({
  name: 'ui',
  initialState,
  reducers: {
    setActiveProject(state, action: PayloadAction<string | null>) {
      state.activeProjectId = action.payload;
      state.selectedAgentId = null;
      state.error = null;
    },
    setActiveView(state, action: PayloadAction<ActiveView>) {
      state.activeView = action.payload;
    },
    setSelectedAgent(state, action: PayloadAction<string | null>) {
      state.selectedAgentId = action.payload;
    },
    setError(state, action: PayloadAction<string | null>) {
      state.error = action.payload;
    },
    setConnectionState(state, action: PayloadAction<ConnectionState>) {
      state.connectionState = action.payload;
    },
    setSidebarOpen(state, action: PayloadAction<boolean>) {
      state.sidebarOpen = action.payload;
    },
    toggleSidebar(state) {
      state.sidebarOpen = !state.sidebarOpen;
    },
  },
});

export const {
  setActiveProject,
  setActiveView,
  setSelectedAgent,
  setError,
  setConnectionState,
  setSidebarOpen,
  toggleSidebar,
} = uiSlice.actions;
export const uiReducer = uiSlice.reducer;
