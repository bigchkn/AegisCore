import { Terminal } from '../components/Terminal';
import { useAppSelector } from '../store/hooks';

export function PaneView() {
  const selectedAgentId = useAppSelector((state) => state.ui.selectedAgentId);
  const agent = useAppSelector((state) =>
    state.agents.items.find((item) => item.agent_id === selectedAgentId),
  );

  if (!selectedAgentId || !agent) {
    return (
      <section className="empty-state">
        <h2>No agent selected</h2>
        <p>Select an agent row to attach to its live pane.</p>
      </section>
    );
  }

  return (
    <section className="pane-view">
      <header className="pane-header">
        <div>
          <h2>{agent.name}</h2>
          <p>{agent.tmux_session}:{agent.tmux_window}.{agent.tmux_pane}</p>
        </div>
        <span className="connection-pill">{agent.cli_provider}</span>
      </header>
      <Terminal agentId={selectedAgentId} />
    </section>
  );
}
