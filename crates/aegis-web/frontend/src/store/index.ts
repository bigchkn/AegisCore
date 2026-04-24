import { configureStore } from '@reduxjs/toolkit';

import { agentsReducer } from './agentsSlice';
import { channelsReducer } from './channelsSlice';
import { projectsReducer } from './projectsSlice';
import { tasksReducer } from './tasksSlice';
import { uiReducer } from './uiSlice';
import { wsMiddleware } from './wsMiddleware';

export const store = configureStore({
  reducer: {
    agents: agentsReducer,
    channels: channelsReducer,
    projects: projectsReducer,
    tasks: tasksReducer,
    ui: uiReducer,
  },
  middleware: (getDefaultMiddleware) => getDefaultMiddleware().concat(wsMiddleware),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
