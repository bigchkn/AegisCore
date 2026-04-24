import { createSlice, type PayloadAction } from '@reduxjs/toolkit';

import type { ChannelKind } from '../types/ChannelKind';
import type { ChannelRecord } from '../types/ChannelRecord';

export type ChannelsState = {
  items: ChannelRecord[];
  loading: boolean;
};

const initialState: ChannelsState = {
  items: [],
  loading: false,
};

const channelsSlice = createSlice({
  name: 'channels',
  initialState,
  reducers: {
    setChannels(state, action: PayloadAction<ChannelRecord[]>) {
      state.items = action.payload;
      state.loading = false;
    },
    setChannelsLoading(state, action: PayloadAction<boolean>) {
      state.loading = action.payload;
    },
    upsertChannel(state, action: PayloadAction<ChannelRecord>) {
      const index = state.items.findIndex((channel) => channel.name === action.payload.name);
      if (index === -1) {
        state.items.push(action.payload);
      } else {
        state.items[index] = action.payload;
      }
    },
    addChannelByEvent(state, action: PayloadAction<{ name: string; kind: ChannelKind }>) {
      const existing = state.items.find((channel) => channel.name === action.payload.name);
      if (existing) {
        existing.kind = action.payload.kind;
        existing.active = true;
      } else {
        state.items.push({
          name: action.payload.name,
          kind: action.payload.kind,
          active: true,
          registered_at: new Date().toISOString(),
          config: null,
        });
      }
    },
    removeChannel(state, action: PayloadAction<string>) {
      state.items = state.items.filter((channel) => channel.name !== action.payload);
    },
  },
});

export const {
  setChannels,
  setChannelsLoading,
  upsertChannel,
  addChannelByEvent,
  removeChannel,
} = channelsSlice.actions;
export const channelsReducer = channelsSlice.reducer;
