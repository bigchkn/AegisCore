import { useState, type FormEvent } from 'react';

import { failoverAgent, killAgent, pauseAgent, resumeAgent, spawnTask } from '../api/thunks';
import { StatusBadge } from '../components/StatusBadge';
import { setActiveView, setSelectedAgent } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';

export function AgentsView() {
  const dispatch = useAppDispatch();
  const agents = useAppSelector((state) => state.agents.items);
  const loading = useAppSelector((state) => state.agents.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [taskPrompt, setTaskPrompt] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [spawnError, setSpawnError] = useState<string | null>(null);

  async function handleSpawn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeProjectId) {
      return;
    }

    const prompt = taskPrompt.trim();
    if (!prompt) {
      setSpawnError('Enter a task prompt before spawning an agent.');
      return;
    }

    setSubmitting(true);
    setSpawnError(null);
    try {
      await dispatch(spawnTask({ projectId: activeProjectId, task: prompt })).unwrap();
      setTaskPrompt('');
    } catch (error) {
      setSpawnError(error instanceof Error ? error.message : 'Unable to spawn agent.');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="agents-view">
      <section className="agent-composer">
        <form className="agent-composer-form" onSubmit={handleSpawn}>
          <div className="agent-composer-copy">
            <h2>Spawn agent</h2>
            <p>Submit a task prompt to create a new agent session.</p>
          </div>
          <textarea
            value={taskPrompt}
            placeholder="Describe the task for the new agent"
            onChange={(event) => setTaskPrompt(event.target.value)}
            rows={3}
            disabled={!activeProjectId || submitting}
          />
          <div className="agent-composer-footer">
            <button type="submit" disabled={!activeProjectId || submitting || taskPrompt.trim().length === 0}>
              {submitting ? 'Spawning...' : 'Spawn Agent'}
            </button>
            {spawnError ? <span className="agent-composer-error">{spawnError}</span> : null}
          </div>
        </form>
      </section>

      {!activeProjectId ? (
        <EmptyPanel title="Select a project" body="Registered projects appear in the sidebar." />
      ) : loading ? (
        <EmptyPanel title="Loading agents" body="Fetching current registry state." />
      ) : agents.length === 0 ? (
        <EmptyPanel title="No agents" body="Spawned sessions will appear here." />
      ) : (
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
      )}
    </div>
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
