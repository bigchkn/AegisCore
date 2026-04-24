import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

import type { ProjectRecord } from './domain';

export type ProjectsState = {
  items: ProjectRecord[];
  loading: boolean;
};

const initialState: ProjectsState = {
  items: [],
  loading: false,
};

const projectsSlice = createSlice({
  name: 'projects',
  initialState,
  reducers: {
    setProjects(state, action: PayloadAction<ProjectRecord[]>) {
      state.items = action.payload;
      state.loading = false;
    },
    setProjectsLoading(state, action: PayloadAction<boolean>) {
      state.loading = action.payload;
    },
    upsertProject(state, action: PayloadAction<ProjectRecord>) {
      const index = state.items.findIndex((project) => project.project_id === action.payload.project_id);
      if (index === -1) {
        state.items.push(action.payload);
      } else {
        state.items[index] = action.payload;
      }
    },
  },
});

export const { setProjects, setProjectsLoading, upsertProject } = projectsSlice.actions;
export const projectsReducer = projectsSlice.reducer;
