import { failoverAgent, killAgent, pauseAgent, resumeAgent } from '../api/thunks';
import { StatusBadge } from '../components/StatusBadge';
import { setActiveView, setSelectedAgent } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';

export function AgentsView() {
  const dispatch = useAppDispatch();
  const agents = useAppSelector((state) => state.agents.items);
  const loading = useAppSelector((state) => state.agents.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);

  if (!activeProjectId) {
    return <EmptyPanel title="Select a project" body="Registered projects appear in the sidebar." />;
  }

  if (loading) {
    return <EmptyPanel title="Loading agents" body="Fetching current registry state." />;
  }

  if (agents.length === 0) {
    return <EmptyPanel title="No agents" body="Spawned sessions will appear here." />;
  }

  return (
    <section className="table-panel">
      <table>
        <thead>
          <tr>
            <th>Name</th>
            <th>Kind</th>
            <th>Status</th>
            <th>Provider</th>
            <th>Task</th>
            <th aria-label="Agent actions" />
          </tr>
        </thead>
        <tbody>
          {agents.map((agent) => (
            <tr
              key={agent.agent_id}
              onClick={() => {
                dispatch(setSelectedAgent(agent.agent_id));
                dispatch(setActiveView('pane'));
              }}
            >
              <td>
                <strong>{agent.name}</strong>
                <span className="subtle">{agent.role}</span>
              </td>
              <td>{agent.kind}</td>
              <td>
                <StatusBadge status={agent.status} />
              </td>
              <td>{agent.cli_provider}</td>
              <td>{agent.task_id ?? 'none'}</td>
              <td>
                <div className="row-actions" onClick={(event) => event.stopPropagation()}>
                  <button
                    type="button"
                    onClick={() =>
                      void dispatch(pauseAgent({ projectId: activeProjectId, agentId: agent.agent_id }))
                    }
                  >
                    Pause
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      void dispatch(resumeAgent({ projectId: activeProjectId, agentId: agent.agent_id }))
                    }
                  >
                    Resume
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      void dispatch(failoverAgent({ projectId: activeProjectId, agentId: agent.agent_id }))
                    }
                  >
                    Failover
                  </button>
                  <button
                    type="button"
                    className="danger"
                    onClick={() =>
                      void dispatch(killAgent({ projectId: activeProjectId, agentId: agent.agent_id }))
                    }
                  >
                    Kill
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function EmptyPanel({ title, body }: { title: string; body: string }) {
  return (
    <section className="empty-state">
      <h2>{title}</h2>
      <p>{body}</p>
    </section>
  );
}
