import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

import type { Task } from '../types/Task';

export type TasksState = {
  items: Task[];
  loading: boolean;
};

const initialState: TasksState = {
  items: [],
  loading: false,
};

const tasksSlice = createSlice({
  name: 'tasks',
  initialState,
  reducers: {
    setTasks(state, action: PayloadAction<Task[]>) {
      state.items = action.payload;
      state.loading = false;
    },
    setTasksLoading(state, action: PayloadAction<boolean>) {
      state.loading = action.payload;
    },
    upsertTask(state, action: PayloadAction<Task>) {
      const index = state.items.findIndex((task) => task.task_id === action.payload.task_id);
      if (index === -1) {
        state.items.push(action.payload);
      } else {
        state.items[index] = action.payload;
      }
    },
    markTaskComplete(state, action: PayloadAction<{ task_id: string; receipt_path: string }>) {
      const task = state.items.find((item) => item.task_id === action.payload.task_id);
      if (task) {
        task.status = 'complete';
        task.receipt_path = action.payload.receipt_path;
        task.completed_at = new Date().toISOString();
      }
    },
    assignTask(state, action: PayloadAction<{ task_id: string; agent_id: string }>) {
      const task = state.items.find((item) => item.task_id === action.payload.task_id);
      if (task) {
        task.assigned_agent_id = action.payload.agent_id;
        task.status = 'active';
      }
    },
  },
});

export const { setTasks, setTasksLoading, upsertTask, markTaskComplete, assignTask } =
  tasksSlice.actions;
export const tasksReducer = tasksSlice.reducer;
