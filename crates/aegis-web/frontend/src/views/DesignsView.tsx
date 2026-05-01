import { useCallback, useEffect, useMemo, useState, type Dispatch, type SetStateAction } from 'react';
import { toast } from 'sonner';

import { api } from '../api/rest';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import { useProjectViewState } from '../lib/useProjectViewState';
import type { DesignDocContent, DesignDocSummary, DesignRefinementDraft } from '../store/domain';
import { useAppSelector } from '../store/hooks';

type RefinementForm = DesignRefinementDraft;

const defaultRefinementForm: RefinementForm = {
  doc_type: 'LLD',
  doc_path: '.aegis/designs/lld/refinement.md',
  doc_description: '',
  bastion_agent_id: '',
  hld_ref: '',
  task_id: '',
  provider: '',
  model: '',
};

export function DesignsView() {
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [docs, setDocs] = useState<DesignDocSummary[]>([]);
  const [selectedPath, setSelectedPath] = useProjectViewState<string | null>(
    activeProjectId,
    'designs.selectedPath',
    null,
    isStringOrNull,
  );
  const [selectedDoc, setSelectedDoc] = useState<DesignDocContent | null>(null);
  const [loading, setLoading] = useState(false);
  const [docLoading, setDocLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refinement, setRefinement] = useState<RefinementForm | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [listCollapsed, setListCollapsed] = useProjectViewState(
    activeProjectId,
    'designs.listCollapsed',
    false,
    isBoolean,
  );

  const loadDocs = useCallback(async () => {
    if (!activeProjectId) return;
    setLoading(true);
    try {
      const items = await api.listDesignDocs(activeProjectId);
      setDocs(items);
      setError(null);
      setSelectedPath((current) =>
        current && items.some((item) => item.path === current) ? current : items[0]?.path ?? null,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [activeProjectId]);

  useEffect(() => {
    void loadDocs();
  }, [loadDocs]);

  useEffect(() => {
    if (!activeProjectId || !selectedPath) {
      setSelectedDoc(null);
      return;
    }

    setDocLoading(true);
    api
      .readDesignDoc(activeProjectId, selectedPath)
      .then((doc) => {
        setSelectedDoc(doc);
        setError(null);
      })
      .catch((err: Error) => setError(err.message))
      .finally(() => setDocLoading(false));
  }, [activeProjectId, selectedPath]);

  const groupedDocs = useMemo(() => {
    const groups = new Map<string, DesignDocSummary[]>();
    for (const doc of docs) {
      const values = groups.get(doc.kind) ?? [];
      values.push(doc);
      groups.set(doc.kind, values);
    }
    return Array.from(groups.entries()).sort(([left], [right]) => left.localeCompare(right));
  }, [docs]);

  const openRefinement = () => {
    setFormError(null);
    setRefinement({
      ...defaultRefinementForm,
      doc_path: selectedDoc?.path ?? defaultRefinementForm.doc_path,
      doc_type: selectedDoc?.kind === 'HLD' ? 'HLD' : 'LLD',
    });
  };

  const saveRefinement = async () => {
    if (!activeProjectId || !refinement) return;
    if (!refinement.doc_path.trim() || !refinement.doc_description.trim()) {
      setFormError('Document path and refinement brief are required.');
      return;
    }

    setSaving(true);
    setFormError(null);
    try {
      const payload: DesignRefinementDraft = {
        doc_type: refinement.doc_type,
        doc_path: refinement.doc_path.trim(),
        doc_description: refinement.doc_description.trim(),
        bastion_agent_id: refinement.bastion_agent_id?.trim() || null,
        hld_ref: refinement.hld_ref?.trim() || null,
        task_id: refinement.task_id?.trim() || null,
        provider: refinement.provider?.trim() || null,
        model: refinement.model?.trim() || null,
      };
      await api.startDesignRefinement(activeProjectId, payload);
      toast.success('Refinement cycle started');
      setRefinement(null);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setFormError(msg);
      toast.error('Failed to start refinement', { description: msg });
    } finally {
      setSaving(false);
    }
  };

  if (!activeProjectId) {
    return <EmptyPanel title="Select a project" body="Design docs are loaded per project." />;
  }

  if (loading && docs.length === 0) {
    return <EmptyPanel title="Loading designs" body="Reading .aegis/designs." />;
  }

  return (
    <section className="designs-view">
      <header className="designs-header">
        <div>
          <h2>Designs</h2>
          <p>{docs.length} documents under .aegis/designs</p>
        </div>
        <div className="taskflow-actions">
          <button
            type="button"
            aria-expanded={!listCollapsed}
            aria-controls="design-doc-list"
            onClick={() => setListCollapsed((collapsed) => !collapsed)}
          >
            {listCollapsed ? 'Show List' : 'Hide List'}
          </button>
          <button type="button" onClick={() => void loadDocs()}>
            Refresh
          </button>
          <button type="button" onClick={openRefinement}>
            New Refinement
          </button>
        </div>
      </header>

      {error ? <div className="banner">{error}</div> : null}

      <div className={`designs-layout${listCollapsed ? ' is-list-collapsed' : ''}`}>
        {listCollapsed ? null : (
          <aside id="design-doc-list" className="designs-list" aria-label="Design documents">
            {groupedDocs.length === 0 ? (
              <p className="muted padding-14">No design documents found.</p>
            ) : (
              groupedDocs.map(([kind, items]) => (
                <div key={kind} className="designs-group">
                  <h3>{kind}</h3>
                  {items.map((doc) => (
                    <button
                      key={doc.path}
                      type="button"
                      className={doc.path === selectedPath ? 'is-active' : ''}
                      onClick={() => setSelectedPath(doc.path)}
                    >
                      <span>{doc.name}</span>
                      <small>{doc.path}</small>
                    </button>
                  ))}
                </div>
              ))
            )}
          </aside>
        )}

        <article className="designs-reader">
          {docLoading ? (
            <p className="muted padding-14">Loading document...</p>
          ) : selectedDoc ? (
            <>
              <div className="designs-reader-header">
                <div>
                  <h3>{selectedDoc.name}</h3>
                  <p>{selectedDoc.path}</p>
                </div>
                <span className="status-pill status-pending">{selectedDoc.kind}</span>
              </div>
              <MarkdownRenderer content={selectedDoc.content} />
            </>
          ) : (
            <p className="muted padding-14">Select a design document.</p>
          )}
        </article>
      </div>

      {refinement ? (
        <RefinementModal
          form={refinement}
          saving={saving}
          error={formError}
          onClose={() => setRefinement(null)}
          onChange={setRefinement}
          onSave={saveRefinement}
        />
      ) : null}
    </section>
  );
}

function RefinementModal({
  form,
  saving,
  error,
  onClose,
  onChange,
  onSave,
}: {
  form: RefinementForm;
  saving: boolean;
  error: string | null;
  onClose: () => void;
  onChange: Dispatch<SetStateAction<RefinementForm | null>>;
  onSave: () => void;
}) {
  const updateForm = (field: keyof RefinementForm, value: string) => {
    onChange((current) => (current ? { ...current, [field]: value } : current));
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
              <h3>New Refinement</h3>
              <p>Spawn a taskflow-designer cycle for a design document.</p>
            </div>
          </div>

          {error ? <div className="task-editor-error">{error}</div> : null}

          <div className="task-editor-grid">
            <label className="task-editor-field">
              <span>Doc Type</span>
              <select value={form.doc_type} onChange={(event) => updateForm('doc_type', event.target.value)}>
                <option value="LLD">LLD</option>
                <option value="HLD">HLD</option>
              </select>
            </label>

            <label className="task-editor-field task-editor-field-wide">
              <span>Document Path</span>
              <input
                value={form.doc_path}
                onChange={(event) => updateForm('doc_path', event.target.value)}
                placeholder=".aegis/designs/lld/example.md"
              />
            </label>

            <label className="task-editor-field task-editor-field-wide">
              <span>Refinement Brief</span>
              <textarea
                value={form.doc_description}
                onChange={(event) => updateForm('doc_description', event.target.value)}
                rows={5}
                placeholder="Describe what the designer should add, clarify, or revise."
              />
            </label>

            <label className="task-editor-field">
              <span>Coordinator ID</span>
              <input
                value={form.bastion_agent_id ?? ''}
                onChange={(event) => updateForm('bastion_agent_id', event.target.value)}
                placeholder="Auto-detect active Bastion"
              />
            </label>

            <label className="task-editor-field">
              <span>Task ID</span>
              <input
                value={form.task_id ?? ''}
                onChange={(event) => updateForm('task_id', event.target.value)}
                placeholder="Optional roadmap task"
              />
            </label>

            <label className="task-editor-field">
              <span>Parent HLD</span>
              <input
                value={form.hld_ref ?? ''}
                onChange={(event) => updateForm('hld_ref', event.target.value)}
                placeholder=".aegis/designs/hld/aegis.md"
              />
            </label>

            <label className="task-editor-field">
              <span>Model</span>
              <input
                value={form.model ?? ''}
                onChange={(event) => updateForm('model', event.target.value)}
                placeholder="Optional model override"
              />
            </label>
          </div>

          <div className="task-editor-footer">
            <button type="button" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" className="task-editor-save" disabled={saving}>
              {saving ? 'Starting...' : 'Start'}
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

function isStringOrNull(value: unknown): value is string | null {
  return typeof value === 'string' || value === null;
}

function isBoolean(value: unknown): value is boolean {
  return typeof value === 'boolean';
}
