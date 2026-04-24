import { useEffect, useState } from 'react';

import { api } from '../api/rest';
import type { TaskflowIndex, TaskflowMilestone } from '../store/domain';
import { useAppSelector } from '../store/hooks';

type MilestoneState = {
  expanded: boolean;
  loading: boolean;
  data: TaskflowMilestone | null;
  error: string | null;
};

export function TaskflowView() {
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [index, setIndex] = useState<TaskflowIndex | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [milestones, setMilestones] = useState<Record<string, MilestoneState>>({});

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

  const entries = Object.entries(index.milestones).sort(([left], [right]) =>
    left.localeCompare(right, undefined, { numeric: true }),
  );

  return (
    <section className="taskflow-view">
      <header className="taskflow-header">
        <div>
          <h2>{index.project.name}</h2>
          <p>Current milestone M{index.project.current_milestone}</p>
        </div>
      </header>
      <div className="taskflow-tree">
        {entries.map(([milestoneId, ref]) => {
          const state = milestones[milestoneId];
          return (
            <div key={milestoneId} className="milestone-node">
              <button type="button" onClick={() => toggleMilestone(milestoneId)}>
                <span>{state?.expanded ? 'v' : '>'}</span>
                <strong>{milestoneId}</strong>
                <em>{ref.status}</em>
              </button>
              {state?.expanded ? (
                <div className="milestone-detail">
                  {state.loading ? <p>Loading milestone</p> : null}
                  {state.error ? <p>{state.error}</p> : null}
                  {state.data ? <MilestoneDetail milestone={state.data} /> : null}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function MilestoneDetail({ milestone }: { milestone: TaskflowMilestone }) {
  return (
    <>
      <div className="milestone-title">
        <span>M{milestone.id}</span>
        <strong>{milestone.name}</strong>
        <em>{milestone.status}</em>
      </div>
      <div className="taskflow-task-list">
        {milestone.tasks.map((task) => (
          <div key={task.id} className="taskflow-task">
            <span className={`task-icon task-${task.status}`}>{symbolForStatus(task.status)}</span>
            <div>
              <strong>{task.id}</strong>
              <p>{task.task}</p>
              {task.notes ? <small>{task.notes}</small> : null}
            </div>
          </div>
        ))}
      </div>
    </>
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
