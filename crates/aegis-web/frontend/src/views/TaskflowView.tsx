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

type ViewMode = 'milestones' | 'bugs';

export function TaskflowView() {
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [index, setIndex] = useState<TaskflowIndex | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [milestones, setMilestones] = useState<Record<string, MilestoneState>>({});
  
  const [viewMode, setViewMode] = useState<ViewMode>('milestones');
  const [showAll, setShowAll] = useState(false);

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

  const loadMilestone = (milestoneId: string) => {
    if (!activeProjectId) return;
    
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

  const toggleMilestone = (milestoneId: string) => {
    const current = milestones[milestoneId];
    if (current?.data) {
      setMilestones((state) => ({
        ...state,
        [milestoneId]: { ...current, expanded: !current.expanded },
      }));
      return;
    }

    loadMilestone(milestoneId);
  };

  // Auto-expand milestones when in Bugs view
  useEffect(() => {
    if (viewMode === 'bugs') {
      if (!index) return;
      
      const ids = Object.keys(index.milestones);
      if (index.backlog) ids.push('backlog');
      
      for (const id of ids) {
        if (!milestones[id]?.data && !milestones[id]?.loading) {
          loadMilestone(id);
        } else if (milestones[id]?.data && !milestones[id].expanded) {
          setMilestones(state => ({
            ...state,
            [id]: { ...state[id], expanded: true }
          }));
        }
      }
    }
  }, [viewMode, index]);

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
      // In Milestone view, hide completed milestones if showAll is false
      if (viewMode === 'milestones' && !showAll) {
        return ref.status !== 'done';
      }
      return true;
    });

  return (
    <section className="taskflow-view">
      <header className="taskflow-header">
        <div className="taskflow-title">
          <h2>{index.project.name}</h2>
          <p>Current milestone M{index.project.current_milestone}</p>
        </div>
        
        <div className="taskflow-controls">
          <div className="segmented-control">
            <button 
              className={viewMode === 'milestones' ? 'is-active' : ''} 
              onClick={() => setViewMode('milestones')}
            >
              Milestones
            </button>
            <button 
              className={viewMode === 'bugs' ? 'is-active' : ''} 
              onClick={() => setViewMode('bugs')}
            >
              Bugs
            </button>
          </div>

          <div className="segmented-control">
            <button 
              className={!showAll ? 'is-active' : ''} 
              onClick={() => setShowAll(false)}
            >
              Active
            </button>
            <button 
              className={showAll ? 'is-active' : ''} 
              onClick={() => setShowAll(true)}
            >
              All
            </button>
          </div>
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
                  <MilestoneDetail 
                    milestone={milestones['backlog'].data} 
                    viewMode={viewMode}
                    showAll={showAll}
                  />
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
                  {state.data ? (
                    <MilestoneDetail 
                      milestone={state.data} 
                      viewMode={viewMode}
                      showAll={showAll}
                    />
                  ) : null}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}

interface MilestoneDetailProps {
  milestone: TaskflowMilestone;
  viewMode: ViewMode;
  showAll: boolean;
}

function MilestoneDetail({ milestone, viewMode, showAll }: MilestoneDetailProps) {
  const tasks = milestone.tasks.filter(t => {
    // 1. Filter by View Mode
    if (viewMode === 'bugs' && t.task_type !== 'bug') {
      return false;
    }
    
    // 2. Filter by Status
    if (!showAll && t.status === 'done') {
      return false;
    }
    
    return true;
  });

  if (tasks.length === 0) {
    if (viewMode === 'bugs') return null; // Hide milestone detail entirely if no matching bugs
    return <p className="muted">No {!showAll ? 'active ' : ''}tasks found.</p>;
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
