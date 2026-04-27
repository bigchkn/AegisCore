import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';

import { Terminal, type TerminalStatus } from '../components/Terminal';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { setActiveView, setSelectedAgent } from '../store/uiSlice';

export function PaneView() {
  const navigate = useNavigate();
  const dispatch = useAppDispatch();
  const selectedAgentId = useAppSelector((state) => state.ui.selectedAgentId);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const agentsLoading = useAppSelector((state) => state.agents.loading);
  const agent = useAppSelector((state) =>
    state.agents.items.find((item) => item.agent_id === selectedAgentId),
  );
  const [terminalStatus, setTerminalStatus] = useState<TerminalStatus>('connecting');

  useEffect(() => {
    if (selectedAgentId && !agent && !agentsLoading) {
      dispatch(setSelectedAgent(null));
      dispatch(setActiveView('agents'));
      navigate(activeProjectId ? `/projects/${activeProjectId}/agents` : '/agents', { replace: true });
    }
  }, [activeProjectId, agent, agentsLoading, dispatch, navigate, selectedAgentId]);

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
        <p>Select an agent row to attach to its live pane.</p>
        <button
          type="button"
          onClick={() => navigate(activeProjectId ? `/projects/${activeProjectId}/agents` : '/agents')}
        >
          Back to agents
        </button>
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
          <span className="connection-pill">{agent.cli_provider}</span>
          <span className="connection-pill">{terminalStatus}</span>
          <button
            type="button"
            onClick={() => {
              dispatch(setSelectedAgent(null));
              dispatch(setActiveView('agents'));
              navigate(activeProjectId ? `/projects/${activeProjectId}/agents` : '/agents');
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
