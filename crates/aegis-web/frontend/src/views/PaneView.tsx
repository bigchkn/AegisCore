import { useEffect, useState } from 'react';
import { useLocation, useNavigate, useParams } from 'react-router-dom';

import { AgentTargetPicker } from '../components/AgentTargetPicker';
import { Terminal, type TerminalStatus } from '../components/Terminal';
import { agentIdFromLocation, agentRoute } from '../lib/agentRoutes';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { setSelectedAgent } from '../store/uiSlice';

export function PaneView() {
  const navigate = useNavigate();
  const location = useLocation();
  const dispatch = useAppDispatch();
  const { projectId: routeProjectId, agentId: routeAgentId } = useParams<{ projectId?: string; agentId?: string }>();
  const agentsLoading = useAppSelector((state) => state.agents.loading);
  const agents = useAppSelector((state) => state.agents.items);
  const selectedAgentId = routeAgentId ?? agentIdFromLocation(location.pathname, location.search);
  const agent = useAppSelector((state) =>
    state.agents.items.find((item) => item.agent_id === selectedAgentId),
  );
  const [terminalStatus, setTerminalStatus] = useState<TerminalStatus>('connecting');

  useEffect(() => {
    dispatch(setSelectedAgent(selectedAgentId ?? null));
  }, [dispatch, selectedAgentId]);

  useEffect(() => {
    if (selectedAgentId && !agent && !agentsLoading) {
      dispatch(setSelectedAgent(null));
      navigate(agentRoute(routeProjectId ?? null, 'pane'), { replace: true });
    }
  }, [agent, agentsLoading, dispatch, navigate, routeProjectId, selectedAgentId]);

  if (agentsLoading && !agent) {
    return (
      <section className="empty-state">
        <h2>Loading agent...</h2>
        <p>Fetching the latest registry state.</p>
      </section>
    );
  }

  if (!selectedAgentId || !agent) {
    return (
      <section className="empty-state">
        <h2>No agent selected</h2>
        <p>Select an agent to open its live pane.</p>
        <AgentTargetPicker
          agents={agents}
          selectedAgentId={selectedAgentId ?? null}
          label="Agent"
          onSelect={(agentId) => {
            if (!agentId) {
              return;
            }
            navigate(agentRoute(routeProjectId ?? null, 'pane', agentId));
          }}
        />
      </section>
    );
  }

  return (
    <section className="pane-view">
      <header className="pane-header">
        <div>
          <h2>{agent.name}</h2>
          <p>
            {agent.tmux_session}:{agent.tmux_window}.{agent.tmux_pane}
          </p>
        </div>
        <div className="row-actions">
          <AgentTargetPicker
            agents={agents}
            selectedAgentId={selectedAgentId}
            label="Agent"
            onSelect={(agentId) => {
              if (!agentId) {
                return;
              }
              navigate(agentRoute(routeProjectId ?? null, 'pane', agentId));
            }}
          />
          <span className="connection-pill">{agent.cli_provider}</span>
          <span className="connection-pill">{terminalStatus}</span>
          <button
            type="button"
            onClick={() => {
              dispatch(setSelectedAgent(null));
              navigate(agentRoute(routeProjectId ?? null, 'pane'));
            }}
          >
            Detach
          </button>
        </div>
      </header>
      <Terminal agentId={selectedAgentId} onStatusChange={setTerminalStatus} />
    </section>
  );
}
