import { useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { AgentTargetPicker } from '../components/AgentTargetPicker';
import { agentRoute } from '../lib/agentRoutes';
import { useAppSelector } from '../store/hooks';

type LogMessage = {
  type: 'line';
  data: string;
};

const LOG_BUFFER_LIMIT = 2000;

export function LogView() {
  const navigate = useNavigate();
  const { projectId: routeProjectId, agentId: routeAgentId } = useParams<{ projectId?: string; agentId?: string }>();
  const agent = useAppSelector((state) =>
    state.agents.items.find((item) => item.agent_id === routeAgentId),
  );
  const agents = useAppSelector((state) => state.agents.items);
  const [lines, setLines] = useState<string[]>([]);
  const [filter, setFilter] = useState('');
  const [follow, setFollow] = useState(true);
  const [connected, setConnected] = useState(false);
  const listRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setLines([]);
    if (!routeAgentId) {
      return;
    }

    const socket = new WebSocket(wsUrl(`/ws/logs/${routeAgentId}?last_n=200`));
    socket.onopen = () => setConnected(true);
    socket.onclose = () => setConnected(false);
    socket.onerror = () => setConnected(false);
    socket.onmessage = (event) => {
      const message = JSON.parse(event.data) as LogMessage;
      if (message.type === 'line') {
        setLines((current) => [...current, message.data].slice(-LOG_BUFFER_LIMIT));
      }
    };

    return () => socket.close();
  }, [routeAgentId]);

  useEffect(() => {
    if (follow) {
      listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
    }
  }, [follow, lines]);

  const filteredLines = useMemo(() => {
    if (!filter.trim()) {
      return lines;
    }

    try {
      const pattern = new RegExp(filter, 'i');
      return lines.filter((line) => pattern.test(line));
    } catch {
      const needle = filter.toLowerCase();
      return lines.filter((line) => line.toLowerCase().includes(needle));
    }
  }, [filter, lines]);

  if (!routeAgentId || !agent) {
    return (
      <section className="empty-state">
        <h2>No agent selected</h2>
        <p>Select an agent to open its logs.</p>
        <AgentTargetPicker
          agents={agents}
          selectedAgentId={routeAgentId ?? null}
          label="Attachable agents"
          onSelect={(agentId) => {
            if (!agentId) {
              return;
            }
            navigate(agentRoute(routeProjectId ?? null, 'logs', agentId));
          }}
        />
      </section>
    );
  }

  return (
    <section className="log-view">
      <header className="log-toolbar">
        <div>
          <h2>{agent.name}</h2>
          <p>{connected ? 'Streaming recorder output' : 'Disconnected'}</p>
        </div>
        <div className="log-controls">
          <AgentTargetPicker
            agents={agents}
            selectedAgentId={routeAgentId}
            label="Agent"
            onSelect={(agentId) => {
              if (!agentId) {
                return;
              }
              navigate(agentRoute(routeProjectId ?? null, 'logs', agentId));
            }}
          />
          <input
            value={filter}
            placeholder="Filter"
            onChange={(event) => setFilter(event.target.value)}
          />
          <label>
            <input
              type="checkbox"
              checked={follow}
              onChange={(event) => setFollow(event.target.checked)}
            />
            Follow
          </label>
        </div>
      </header>
      <div ref={listRef} className="log-lines">
        {filteredLines.map((line, index) => (
          <div key={`${index}-${line}`} className="log-line">
            {line}
          </div>
        ))}
      </div>
    </section>
  );
}

function wsUrl(path: string) {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${protocol}://${window.location.host}${path}`;
}
