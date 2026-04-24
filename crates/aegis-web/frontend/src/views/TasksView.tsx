import { useMemo, useState } from 'react';

import { setActiveView, setSelectedAgent } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import type { TaskStatus } from '../types/TaskStatus';

const tabs: Array<'all' | TaskStatus> = ['all', 'queued', 'active', 'complete', 'failed'];

export function TasksView() {
  const dispatch = useAppDispatch();
  const tasks = useAppSelector((state) => state.tasks.items);
  const loading = useAppSelector((state) => state.tasks.loading);
  const [activeTab, setActiveTab] = useState<'all' | TaskStatus>('all');

  const filteredTasks = useMemo(
    () => (activeTab === 'all' ? tasks : tasks.filter((task) => task.status === activeTab)),
    [activeTab, tasks],
  );

  if (loading) {
    return <EmptyPanel title="Loading tasks" body="Fetching task registry state." />;
  }

  return (
    <section className="table-panel">
      <div className="view-toolbar">
        <div className="segmented-control">
          {tabs.map((tab) => (
            <button
              key={tab}
              type="button"
              className={tab === activeTab ? 'is-active' : ''}
              onClick={() => setActiveTab(tab)}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>
      {filteredTasks.length === 0 ? (
        <div className="inline-empty">No matching tasks</div>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Task</th>
              <th>Status</th>
              <th>Assigned Agent</th>
              <th>Created</th>
              <th>Receipt</th>
            </tr>
          </thead>
          <tbody>
            {filteredTasks.map((task) => (
              <tr key={task.task_id}>
                <td>
                  <strong>{task.description}</strong>
                  <span className="subtle">{task.task_id}</span>
                </td>
                <td>
                  <span className={`status-badge task-${task.status}`}>{task.status}</span>
                </td>
                <td>
                  {task.assigned_agent_id ? (
                    <button
                      type="button"
                      className="link-button"
                      onClick={() => {
                        dispatch(setSelectedAgent(task.assigned_agent_id));
                        dispatch(setActiveView('pane'));
                      }}
                    >
                      {task.assigned_agent_id}
                    </button>
                  ) : (
                    'none'
                  )}
                </td>
                <td>{formatDate(task.created_at)}</td>
                <td>{task.receipt_path ?? 'none'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
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

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
}
