import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import { toast } from 'sonner';

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

type TaskEditorMode = 'create' | 'edit';

type TaskEditorForm = {
  id: string;
  task: string;
  taskType: TaskType;
  status: string;
  crateName: string;
  notes: string;
  targetMilestoneId: string;
};

type TaskEditorState = {
  mode: TaskEditorMode;
  sourceMilestoneId: string;
  sourceMilestoneName: string;
  taskUid?: string;
  form: TaskEditorForm;
};

type MilestoneEditorForm = {
  id: string;
  name: string;
  lld: string;
};

export function TaskflowView() {
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [index, setIndex] = useState<TaskflowIndex | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [milestones, setMilestones] = useState<Record<string, MilestoneState>>({});
  const [editor, setEditor] = useState<TaskEditorState | null>(null);
  const [milestoneEditor, setMilestoneEditor] = useState<MilestoneEditorForm | null>(null);
  const [editorSaving, setEditorSaving] = useState(false);
  const [editorError, setEditorError] = useState<string | null>(null);
  const [mutationWarning, setMutationWarning] = useState<string | null>(null);

  const [viewMode, setViewMode] = useState<ViewMode>('milestones');
  const [showAll, setShowAll] = useState(false);

  const refreshTaskflow = useCallback(async () => {
    if (!activeProjectId) return;

    setIndex(null);
    setMilestones({});
    setLoading(true);
    try {
      const value = await api.taskflowStatus(activeProjectId);
      setIndex(value);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [activeProjectId]);

  useEffect(() => {
    void refreshTaskflow();
  }, [refreshTaskflow]);

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

  const currentMilestoneId = index ? `M${index.project.current_milestone}` : 'backlog';

  const defaultTaskForm = useCallback(
    (mode: 'bug' | 'task'): TaskEditorForm => ({
      id: '',
      task: '',
      taskType: mode === 'bug' ? 'bug' : 'feature',
      status: 'pending',
      crateName: '',
      notes: '',
      targetMilestoneId: mode === 'bug' ? 'backlog' : currentMilestoneId,
    }),
    [currentMilestoneId],
  );

  const openCreateEditor = (mode: 'bug' | 'task') => {
    setEditorError(null);
    setMutationWarning(null);
    setEditor({
      mode: 'create',
      sourceMilestoneId: mode === 'bug' ? 'backlog' : currentMilestoneId,
      sourceMilestoneName: mode === 'bug' ? 'Backlog' : currentMilestoneId,
      form: defaultTaskForm(mode),
    });
  };

  const openMilestoneEditor = () => {
    setEditorError(null);
    setMutationWarning(null);
    setMilestoneEditor({
      id: '',
      name: '',
      lld: '',
    });
  };

  const openEditEditor = (
    task: TaskflowMilestone['tasks'][number],
    sourceMilestoneId: string,
    sourceMilestoneName: string,
  ) => {
    setEditorError(null);
    setMutationWarning(null);
    setEditor({
      mode: 'edit',
      sourceMilestoneId,
      sourceMilestoneName,
      taskUid: task.uid,
      form: {
        id: task.id,
        task: task.task,
        taskType: task.task_type,
        status: task.status,
        crateName: task.crate_name ?? '',
        notes: task.notes ?? '',
        targetMilestoneId: sourceMilestoneId,
      },
    });
  };

  const closeEditor = () => {
    setEditor(null);
    setMilestoneEditor(null);
    setEditorError(null);
    setEditorSaving(false);
  };

  const saveMilestone = async () => {
    if (!activeProjectId || !milestoneEditor) return;

    const { id, name, lld } = milestoneEditor;
    if (!id.trim() || !name.trim()) {
      setEditorError('Milestone ID and Name are required.');
      return;
    }

    setEditorSaving(true);
    setEditorError(null);

    try {
      await api.taskflowCreateMilestone(activeProjectId, id.trim(), name.trim(), lld.trim() || undefined);
      toast.success(`Milestone ${id.trim()} created successfully`);
      await refreshTaskflow();
      closeEditor();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setEditorError(msg);
      toast.error('Failed to create milestone', { description: msg });
    } finally {
      setEditorSaving(false);
    }
  };

  const saveEditor = async () => {
    if (!activeProjectId || !editor) return;

    const task = editor.form.task.trim();
    if (!task) {
      setEditorError('Task text is required.');
      return;
    }

    const targetMilestoneId = editor.form.targetMilestoneId.trim();
    const normalizedTarget = targetMilestoneId || editor.sourceMilestoneId;
    const payload = {
      id: editor.form.id.trim() || undefined,
      task,
      task_type: editor.form.taskType,
      status: editor.form.status,
      crate_name: editor.form.crateName.trim() || null,
      notes: editor.form.notes.trim() || null,
      target_milestone_id: normalizedTarget,
    };

    setEditorSaving(true);
    setEditorError(null);
    setMutationWarning(null);

    try {
      let notifyWarning: string | null = null;
      if (editor.mode === 'create') {
        const response = await api.taskflowCreateTask(activeProjectId, normalizedTarget, payload);
        notifyWarning = response.warning ?? null;
        toast.success('Task created successfully');
      } else if (!editor.taskUid) {
        throw new Error('Missing task uid.');
      } else {
        const response = await api.taskflowUpdateTask(activeProjectId, editor.sourceMilestoneId, editor.taskUid, payload);
        notifyWarning = response.warning ?? null;
        toast.success('Task updated successfully');
      }

      setMutationWarning(notifyWarning);
      await refreshTaskflow();
      if (normalizedTarget) {
        loadMilestone(normalizedTarget);
      }
      if (editor.mode === 'edit' && editor.sourceMilestoneId !== normalizedTarget) {
        loadMilestone(editor.sourceMilestoneId);
      }
      closeEditor();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setEditorError(msg);
      toast.error('Operation failed', { description: msg });
    } finally {
      setEditorSaving(false);
    }
  };

  // Auto-expand and load all milestones when in Bugs view
  useEffect(() => {
    if (viewMode === 'bugs') {
      if (!index) return;
      
      const ids = Object.keys(index.milestones);
      if (index.project.backlog) ids.push('backlog');
      
      for (const id of ids) {
        if (!milestones[id]?.data && !milestones[id]?.loading) {
          loadMilestone(id);
        }
      }
    }
  }, [viewMode, index]);

  // Aggregate all tasks for flat views
  const allLoadedTasks = useMemo(() => {
    const tasks: Array<{ task: TaskflowMilestone['tasks'][0]; milestoneId: string; milestoneName: string }> = [];
    
    // Check backlog
    const backlog = milestones['backlog']?.data;
    if (backlog) {
      for (const t of backlog.tasks) {
        tasks.push({ task: t, milestoneId: 'backlog', milestoneName: 'Backlog' });
      }
    }

    // Check milestones
    for (const [id, mState] of Object.entries(milestones)) {
      if (id === 'backlog' || !mState.data) continue;
      for (const t of mState.data.tasks) {
        tasks.push({ task: t, milestoneId: id, milestoneName: mState.data.name });
      }
    }

    return tasks;
  }, [milestones]);

  const filteredTasks = useMemo(() => {
    return allLoadedTasks.filter(({ task }) => {
      if (viewMode === 'bugs' && task.task_type !== 'bug') return false;
      if (!showAll && task.status === 'done') return false;
      return true;
    });
  }, [allLoadedTasks, viewMode, showAll]);

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
      if (viewMode === 'milestones' && !showAll) {
        return ref.status !== 'done';
      }
      return true;
    });

  const milestoneOptions = [
    { id: 'backlog', label: 'Backlog' },
    ...Object.entries(index.milestones)
      .sort(([left], [right]) => left.localeCompare(right, undefined, { numeric: true }))
      .map(([milestoneId, ref]) => ({ id: milestoneId, label: `${milestoneId} - ${ref.name}` })),
  ];

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

          <div className="taskflow-actions">
            <button type="button" onClick={() => openMilestoneEditor()}>
              New Milestone
            </button>
            <button type="button" onClick={() => openCreateEditor('bug')}>
              New Bug
            </button>
            <button type="button" onClick={() => openCreateEditor('task')}>
              New Task
            </button>
          </div>
        </div>
      </header>

      {mutationWarning ? <div className="banner">{mutationWarning}</div> : null}

      <div className="taskflow-tree">
        {viewMode === 'bugs' ? (
          <div className="taskflow-flat-list">
            {loadingAnyMilestone(milestones) && filteredTasks.length === 0 ? (
              <p className="muted padding-14">Loading bugs...</p>
            ) : filteredTasks.length === 0 ? (
              <p className="muted padding-14">No {showAll ? '' : 'active '}bugs found.</p>
            ) : (
              <div className="taskflow-task-list padding-14">
                {filteredTasks.map(({ task, milestoneId, milestoneName }) => (
                  <TaskItem 
                    key={task.uid}
                    task={task} 
                    context={milestoneName}
                    sourceMilestoneId={milestoneId}
                    sourceMilestoneName={milestoneName}
                    onEdit={openEditEditor}
                  />
                ))}
              </div>
            )}
          </div>
        ) : (
          <>
            {/* Tree View: Global Backlog */}
            {index.project.backlog ? (
              <div className="milestone-node">                <button type="button" onClick={() => toggleMilestone('backlog')}>
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
                        showAll={showAll}
                        sourceMilestoneId="backlog"
                        sourceMilestoneName="Backlog"
                        onEdit={openEditEditor}
                      />
                    ) : null}
                  </div>
                ) : null}
              </div>
            ) : null}

            {/* Tree View: Milestones */}
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
                          showAll={showAll}
                          sourceMilestoneId={milestoneId}
                          sourceMilestoneName={ref.name}
                          onEdit={openEditEditor}
                        />
                      ) : null}
                    </div>
                  ) : null}
                </div>
              );
            })}
          </>
        )}
      </div>

      {editor ? (
        <TaskEditorModal
          editor={editor}
          milestoneOptions={milestoneOptions}
          saving={editorSaving}
          error={editorError}
          onClose={closeEditor}
          onChange={setEditor}
          onSave={saveEditor}
        />
      ) : null}

      {milestoneEditor ? (
        <MilestoneEditorModal
          form={milestoneEditor}
          saving={editorSaving}
          error={editorError}
          onClose={closeEditor}
          onChange={setMilestoneEditor}
          onSave={saveMilestone}
        />
      ) : null}
    </section>
  );
}

function loadingAnyMilestone(milestones: Record<string, MilestoneState>) {
  return Object.values(milestones).some(m => m.loading);
}

function MilestoneDetail({
  milestone,
  showAll,
  sourceMilestoneId,
  sourceMilestoneName,
  onEdit,
}: {
  milestone: TaskflowMilestone;
  showAll: boolean;
  sourceMilestoneId: string;
  sourceMilestoneName: string;
  onEdit: (
    task: TaskflowMilestone['tasks'][number],
    sourceMilestoneId: string,
    sourceMilestoneName: string,
  ) => void;
}) {
  const tasks = milestone.tasks.filter(t => showAll || t.status !== 'done');

  if (tasks.length === 0) {
    return <p className="muted">No {!showAll ? 'active ' : ''}tasks found.</p>;
  }

  return (
    <div className="taskflow-task-list">
      {tasks.map((task) => (
        <TaskItem
          key={task.uid}
          task={task}
          sourceMilestoneId={sourceMilestoneId}
          sourceMilestoneName={sourceMilestoneName}
          onEdit={onEdit}
        />
      ))}
    </div>
  );
}

function TaskItem({
  task,
  context,
  sourceMilestoneId,
  sourceMilestoneName,
  onEdit,
}: {
  task: TaskflowMilestone['tasks'][0];
  context?: string;
  sourceMilestoneId: string;
  sourceMilestoneName: string;
  onEdit: (
    task: TaskflowMilestone['tasks'][number],
    sourceMilestoneId: string,
    sourceMilestoneName: string,
  ) => void;
}) {
  return (
    <div className="taskflow-task" data-status={task.status}>
      <span className={`task-icon task-${task.status}`}>{symbolForStatus(task.status)}</span>
      <div className="task-content">
        <div className="task-row">
          <TaskTypeBadge type={task.task_type} />
          <strong>{task.id}</strong>
          <p>{task.task}</p>
          {context ? <span className="task-context-label">{context}</span> : null}
          <button
            type="button"
            className="task-edit-button"
            onClick={() => onEdit(task, sourceMilestoneId, sourceMilestoneName)}
          >
            Edit
          </button>
        </div>
        {task.notes ? <small className="task-notes">{task.notes}</small> : null}
      </div>
    </div>
  );
}

function TaskTypeBadge({ type }: { type: TaskType }) {
  const label = type.charAt(0).toUpperCase() + type.slice(1);
  return <span className={`task-type-badge type-${type}`}>{label}</span>;
}

function TaskEditorModal({
  editor,
  milestoneOptions,
  saving,
  error,
  onClose,
  onChange,
  onSave,
}: {
  editor: TaskEditorState;
  milestoneOptions: Array<{ id: string; label: string }>;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onChange: Dispatch<SetStateAction<TaskEditorState | null>>;
  onSave: () => void;
}) {
  const updateForm = (field: keyof TaskEditorForm, value: string) => {
    onChange((current) =>
      current
        ? {
            ...current,
            form: {
              ...current.form,
              [field]: value as never,
            },
          }
        : current,
    );
  };

  return (
    <div className="task-editor-backdrop" onClick={onClose} role="presentation">
      <div className="task-editor-modal" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
        <form
          className="task-editor-form"
          onSubmit={(event) => {
            event.preventDefault();
            onSave();
          }}
        >
          <div className="task-editor-header">
            <div>
              <h3>{editor.mode === 'create' ? 'New Task' : 'Edit Task'}</h3>
              <p>{editor.mode === 'create' ? 'Create a task or bug from Taskflow.' : editor.sourceMilestoneName}</p>
            </div>
            <button type="submit" className="task-editor-save" disabled={saving}>
              {saving ? 'Saving...' : 'Save'}
            </button>
          </div>

          {error ? <div className="task-editor-error">{error}</div> : null}

          <div className="task-editor-grid">
            <label className="task-editor-field">
              <span>Task ID</span>
              <input
                value={editor.form.id}
                onChange={(event) => updateForm('id', event.target.value)}
                placeholder={editor.mode === 'create' ? 'Optional' : 'Required'}
              />
            </label>

            <label className="task-editor-field task-editor-field-wide">
              <span>Task</span>
              <input
                value={editor.form.task}
                onChange={(event) => updateForm('task', event.target.value)}
                placeholder="Describe the task"
              />
            </label>

            <label className="task-editor-field">
              <span>Type</span>
              <select
                value={editor.form.taskType}
                onChange={(event) => updateForm('taskType', event.target.value)}
              >
                <option value="feature">Feature</option>
                <option value="bug">Bug</option>
                <option value="maintenance">Maintenance</option>
              </select>
            </label>

            <label className="task-editor-field">
              <span>Status</span>
              <select
                value={editor.form.status}
                onChange={(event) => updateForm('status', event.target.value)}
              >
                <option value="pending">Pending</option>
                <option value="in-progress">In progress</option>
                <option value="done">Done</option>
                <option value="blocked">Blocked</option>
                <option value="lld-in-progress">LLD in progress</option>
                <option value="lld-done">LLD done</option>
              </select>
            </label>

            <label className="task-editor-field">
              <span>Target</span>
              <select
                value={editor.form.targetMilestoneId}
                onChange={(event) => updateForm('targetMilestoneId', event.target.value)}
              >
                {milestoneOptions.map((option) => (
                  <option key={option.id} value={option.id}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>

            <label className="task-editor-field">
              <span>Crate</span>
              <input
                value={editor.form.crateName}
                onChange={(event) => updateForm('crateName', event.target.value)}
                placeholder="Optional crate name"
              />
            </label>

            <label className="task-editor-field task-editor-field-wide">
              <span>Notes</span>
              <textarea
                value={editor.form.notes}
                onChange={(event) => updateForm('notes', event.target.value)}
                rows={5}
                placeholder="Optional notes"
              />
            </label>
          </div>

          <div className="task-editor-footer">
            <button type="button" onClick={onClose}>
              Cancel
            </button>
          </div>
        </form>
      </div>
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

function symbolForStatus(status: string) {
  if (status === 'done') return '✓';
  if (status === 'in-progress' || status === 'lld-in-progress') return '●';
  if (status === 'blocked') return '!';
  return '○';
}

function MilestoneEditorModal({
  form,
  saving,
  error,
  onClose,
  onChange,
  onSave,
}: {
  form: MilestoneEditorForm;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onChange: Dispatch<SetStateAction<MilestoneEditorForm | null>>;
  onSave: () => void;
}) {
  const updateForm = (field: keyof MilestoneEditorForm, value: string) => {
    onChange((current) =>
      current
        ? {
            ...current,
            [field]: value,
          }
        : current,
    );
  };

  return (
    <div className="task-editor-backdrop" onClick={onClose} role="presentation">
      <div
        className="task-editor-modal"
        role="dialog"
        aria-modal="true"
        onClick={(event) => event.stopPropagation()}
      >
        <form
          className="task-editor-form"
          onSubmit={(event) => {
            event.preventDefault();
            onSave();
          }}
        >
          <div className="task-editor-header">
            <div>
              <h3>New Milestone</h3>
              <p>Create a new milestone file in the roadmap.</p>
            </div>
            <button type="submit" className="task-editor-save" disabled={saving}>
              {saving ? 'Creating...' : 'Create'}
            </button>
          </div>

          {error ? <div className="task-editor-error">{error}</div> : null}

          <div className="task-editor-grid">
            <label className="task-editor-field">
              <span>Milestone ID</span>
              <input
                value={form.id}
                onChange={(event) => updateForm('id', event.target.value)}
                placeholder="e.g. M35"
                autoFocus
              />
            </label>

            <label className="task-editor-field task-editor-field-wide">
              <span>Name</span>
              <input
                value={form.name}
                onChange={(event) => updateForm('name', event.target.value)}
                placeholder="Milestone title"
              />
            </label>

            <label className="task-editor-field task-editor-field-wide">
              <span>LLD Path</span>
              <input
                value={form.lld}
                onChange={(event) => updateForm('lld', event.target.value)}
                placeholder="e.g. lld/my-feature.md (optional)"
              />
            </label>
          </div>

          <div className="task-editor-footer">
            <button type="button" onClick={onClose}>
              Cancel
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
