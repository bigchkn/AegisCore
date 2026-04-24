import type { Middleware } from '@reduxjs/toolkit';

import { updateAgentStatus, removeAgent } from './agentsSlice';
import { addChannelByEvent, removeChannel } from './channelsSlice';
import { setConnectionState, setError } from './uiSlice';
import { assignTask, markTaskComplete } from './tasksSlice';
import type { AegisEvent } from '../types/AegisEvent';

const WS_EVENTS_URL =
  `${window.location.protocol === 'https:' ? 'wss' : 'ws'}://${window.location.host}/ws/events`;

export const wsMiddleware: Middleware = (store) => {
  let socket: WebSocket | null = null;
  let reconnectTimer: number | null = null;
  let reconnectAttempt = 0;

  const scheduleReconnect = () => {
    const delay = Math.min(30_000, 1000 * 2 ** reconnectAttempt);
    reconnectAttempt += 1;
    reconnectTimer = window.setTimeout(connect, delay);
  };

  const handleEvent = (event: AegisEvent) => {
    switch (event.type) {
      case 'agent_status_changed':
        store.dispatch(updateAgentStatus({ agent_id: event.agent_id, status: event.new_status }));
        break;
      case 'agent_terminated':
        store.dispatch(removeAgent(event.agent_id));
        break;
      case 'task_complete':
        store.dispatch(markTaskComplete({ task_id: event.task_id, receipt_path: event.receipt_path }));
        break;
      case 'task_assigned':
        store.dispatch(assignTask({ task_id: event.task_id, agent_id: event.agent_id }));
        break;
      case 'channel_added':
        store.dispatch(addChannelByEvent({ name: event.channel_name, kind: event.channel_type }));
        break;
      case 'channel_removed':
        store.dispatch(removeChannel(event.channel_name));
        break;
      case 'watchdog_alert':
        store.dispatch(setError(`Watchdog requested ${event.action}`));
        break;
      case 'system_notification':
        store.dispatch(setError(event.message));
        break;
      case 'failover_initiated':
        store.dispatch(
          setError(`Failover started for ${event.agent_id}: ${event.from_provider} -> ${event.to_provider}`),
        );
        break;
      case 'agent_spawned':
        break;
    }
  };

  const connect = () => {
    if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
      return;
    }

    store.dispatch(setConnectionState('connecting'));
    socket = new WebSocket(WS_EVENTS_URL);

    socket.onopen = () => {
      reconnectAttempt = 0;
      store.dispatch(setConnectionState('connected'));
      store.dispatch(setError(null));
    };

    socket.onmessage = (message) => {
      try {
        handleEvent(JSON.parse(message.data) as AegisEvent);
      } catch {
        store.dispatch(setError('Received an invalid event payload'));
      }
    };

    socket.onclose = () => {
      store.dispatch(setConnectionState('disconnected'));
      scheduleReconnect();
    };

    socket.onerror = () => {
      store.dispatch(setError('Event stream disconnected'));
      socket?.close();
    };
  };

  window.setTimeout(connect, 0);

  return (next) => (action) => {
    if (reconnectTimer && isAction(action) && action.type === 'ui/setActiveProject') {
      window.clearTimeout(reconnectTimer);
      reconnectTimer = null;
      connect();
    }
    return next(action);
  };
};

function isAction(action: unknown): action is { type: string } {
  return (
    typeof action === 'object' &&
    action !== null &&
    'type' in action &&
    typeof action.type === 'string'
  );
}
