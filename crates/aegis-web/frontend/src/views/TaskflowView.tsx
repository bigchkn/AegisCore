import { useEffect, useState } from 'react';

import { api } from '../api/rest';
import type { TaskflowIndex, TaskflowMilestone, TaskType } from '../store/domain';
import { useAppSelector } from '../store/hooks';

type MilestoneState = {
  expanded: boolean;
  loading: boolean;
  data: TaskflowMilestone | null;
  error: string | null;
};

type Filter = 'all' | 'incomplete' | 'bugs';

export function TaskflowView() {
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [index, setIndex] = useState<TaskflowIndex | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [milestones, setMilestones] = useState<Record<string, MilestoneState>>({});
  const [filter, setFilter] = useState<Filter>('incomplete');

  useEffect(() => {
    setIndex(null);
    setMilestones({});
    if (!activeProjectId) {
      return;
    }

    setLoading(true);
    api
      .taskflowStatus(activeProjectId)
      .then((value) => {
        setIndex(value);
        setError(null);
      })
      .catch((err: Error) => setError(err.message))
      .finally(() => setLoading(false));
  }, [activeProjectId]);

  const toggleMilestone = (milestoneId: string) => {
    if (!activeProjectId) {
      return;
    }

    const current = milestones[milestoneId];
    if (current?.data) {
      setMilestones((state) => ({
        ...state,
        [milestoneId]: { ...current, expanded: !current.expanded },
      }));
      return;
    }

    setMilestones((state) => ({
      ...state,
      [milestoneId]: { expanded: true, loading: true, data: null, error: null },
    }));

    api
      .taskflowMilestone(activeProjectId, milestoneId)
      .then((data) =>
        setMilestones((state) => ({
          ...state,
          [milestoneId]: { expanded: true, loading: false, data, error: null },
        })),
      )
      .catch((err: Error) =>
        setMilestones((state) => ({
          ...state,
          [milestoneId]: { expanded: true, loading: false, data: null, error: err.message },
        })),
      );
  };

  if (!activeProjectId) {
    return <EmptyPanel title="Select a project" body="Taskflow status is loaded per project." />;
  }

  if (loading) {
    return <EmptyPanel title="Loading taskflow" body="Reading roadmap index." />;
  }

  if (error) {
    return <EmptyPanel title="Taskflow unavailable" body={error} />;
  }

  if (!index) {
    return <EmptyPanel title="No taskflow" body="No roadmap index has been loaded." />;
  }

  const milestoneEntries = Object.entries(index.milestones)
    .sort(([left], [right]) => left.localeCompare(right, undefined, { numeric: true }))
    .filter(([_, ref]) => {
      if (filter === 'incomplete') {
        return ref.status !== 'done';
      }
      return true; // Show all for 'all' or 'bugs' (bugs might be in done milestones)
    });

  return (
    <section className="taskflow-view">
      <header className="taskflow-header">
        <div className="taskflow-title">
          <h2>{index.project.name}</h2>
          <p>Current milestone M{index.project.current_milestone}</p>
        </div>
        <div className="taskflow-filters">
          <button 
            className={filter === 'incomplete' ? 'filter-btn is-active' : 'filter-btn'} 
            onClick={() => setFilter('incomplete')}
          >
            Incomplete
          </button>
          <button 
            className={filter === 'bugs' ? 'filter-btn is-active' : 'filter-btn'} 
            onClick={() => setFilter('bugs')}
          >
            Bugs
          </button>
          <button 
            className={filter === 'all' ? 'filter-btn is-active' : 'filter-btn'} 
            onClick={() => setFilter('all')}
          >
            All
          </button>
        </div>
      </header>

      <div className="taskflow-tree">
        {/* Global Backlog */}
        {index.backlog ? (
          <div className="milestone-node">
            <button type="button" onClick={() => toggleMilestone('backlog')}>
              <span>{milestones['backlog']?.expanded ? '▼' : '▶'}</span>
              <strong>Backlog</strong>
              <em>Global</em>
            </button>
            {milestones['backlog']?.expanded ? (
              <div className="milestone-detail">
                {milestones['backlog'].loading ? <p className="muted">Loading backlog...</p> : null}
                {milestones['backlog'].error ? <p className="error">{milestones['backlog'].error}</p> : null}
                {milestones['backlog'].data ? (
                  <MilestoneDetail milestone={milestones['backlog'].data} filter={filter} />
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}

        {/* Milestones */}
        {milestoneEntries.map(([milestoneId, ref]) => {
          const state = milestones[milestoneId];
          return (
            <div key={milestoneId} className="milestone-node">
              <button type="button" onClick={() => toggleMilestone(milestoneId)}>
                <span>{state?.expanded ? '▼' : '▶'}</span>
                <div className="milestone-ref-info">
                  <strong>{milestoneId}</strong>
                  <span className="milestone-name">{ref.name}</span>
                </div>
                <em className={`status-pill status-${ref.status}`}>{ref.status}</em>
              </button>
              {state?.expanded ? (
                <div className="milestone-detail">
                  {state.loading ? <p className="muted">Loading milestone...</p> : null}
                  {state.error ? <p className="error">{state.error}</p> : null}
                  {state.data ? <MilestoneDetail milestone={state.data} filter={filter} /> : null}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function MilestoneDetail({ milestone, filter }: { milestone: TaskflowMilestone; filter: Filter }) {
  const tasks = milestone.tasks.filter(t => {
    if (filter === 'incomplete') {
      return t.status !== 'done';
    }
    if (filter === 'bugs') {
      return t.task_type === 'bug';
    }
    return true;
  });

  if (tasks.length === 0) {
    if (filter === 'bugs') return null; // Hide milestone detail entirely if no bugs
    return <p className="muted">No {filter === 'incomplete' ? 'pending ' : ''}tasks found.</p>;
  }

  return (
    <div className="taskflow-task-list">
      {tasks.map((task) => (
        <div key={task.id} className="taskflow-task">
          <span className={`task-icon task-${task.status}`}>{symbolForStatus(task.status)}</span>
          <div className="task-content">
            <div className="task-row">
              <TaskTypeBadge type={task.task_type} />
              <strong>{task.id}</strong>
              <p>{task.task}</p>
            </div>
            {task.notes ? <small className="task-notes">{task.notes}</small> : null}
          </div>
        </div>
      ))}
    </div>
  );
}

function TaskTypeBadge({ type }: { type: TaskType }) {
  const label = type.charAt(0).toUpperCase() + type.slice(1);
  return <span className={`task-type-badge type-${type}`}>{label}</span>;
}

function EmptyPanel({ title, body }: { title: string; body: string }) {
  return (
    <section className="empty-state">
      <h2>{title}</h2>
      <p>{body}</p>
    </section>
  );
}

function symbolForStatus(status: string) {
  if (status === 'done') {
    return '✓';
  }
  if (status === 'in-progress' || status === 'lld-in-progress') {
    return '●';
  }
  if (status === 'blocked') {
    return '!';
  }
  return '○';
}
