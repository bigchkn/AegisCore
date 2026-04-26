import { useEffect, useState } from 'react';
import { api } from '../api/rest';
import type { ClarificationRequest } from '../store/domain';
import { useAppSelector } from '../store/hooks';

export function ClarificationsView() {
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const [requests, setRequests] = useState<ClarificationRequest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!activeProjectId) return;

    let mounted = true;

    async function fetchRequests() {
      try {
        const data = await api.clarifyList(activeProjectId!);
        if (mounted) {
          // Show open requests first, sorted by priority and date
          setRequests(data.filter(r => r.status === 'open').sort((a, b) => {
            if (b.priority !== a.priority) return b.priority - a.priority;
            return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
          }));
          setError(null);
        }
      } catch (err) {
        if (mounted) setError(err instanceof Error ? err.message : 'Failed to fetch clarifications');
      } finally {
        if (mounted) setLoading(false);
      }
    }

    fetchRequests();
    const interval = setInterval(fetchRequests, 5000);

    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, [activeProjectId]);

  const handleAnswered = (requestId: string) => {
    setRequests(prev => prev.filter(r => r.request_id !== requestId));
  };

  if (!activeProjectId) {
    return <EmptyPanel title="Select a project" body="Select a project to view pending clarifications." />;
  }

  if (loading && requests.length === 0) {
    return <EmptyPanel title="Loading clarifications" body="Fetching pending human requests..." />;
  }

  if (error && requests.length === 0) {
    return <EmptyPanel title="Error" body={error} />;
  }

  return (
    <div className="clarifications-view">
      <header className="view-header">
        <h2>Clarifications</h2>
        <p>Pending human input requests from autonomous agents.</p>
      </header>

      {requests.length === 0 ? (
        <EmptyPanel title="No pending clarifications" body="Agents are making progress autonomously." />
      ) : (
        <div className="clarification-list">
          {requests.map(request => (
            <ClarificationCard 
              key={request.request_id} 
              request={request} 
              projectId={activeProjectId}
              onAnswered={() => handleAnswered(request.request_id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function ClarificationCard({ 
  request, 
  projectId,
  onAnswered 
}: { 
  request: ClarificationRequest; 
  projectId: string;
  onAnswered: () => void;
}) {
  const [answer, setAnswer] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!answer.trim()) return;

    setSubmitting(true);
    setError(null);
    try {
      await api.clarifyAnswer(projectId, request.request_id, answer.trim(), {}, 'human_tui'); // Using tui source as placeholder or system
      onAnswered();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to submit answer');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="clarification-card">
      <div className="card-header">
        <span className="agent-id">Agent: {request.agent_id.slice(0, 8)}</span>
        <span className={`priority-tag priority-${request.priority}`}>P{request.priority}</span>
        <span className="timestamp">{new Date(request.created_at).toLocaleString()}</span>
      </div>
      <div className="card-body">
        <p className="question">{request.question}</p>
        {request.task_id && <div className="task-ref">Task: {request.task_id}</div>}
      </div>
      <form className="card-footer" onSubmit={handleSubmit}>
        <textarea 
          placeholder="Type your answer here..."
          value={answer}
          onChange={(e) => setAnswer(e.target.value)}
          disabled={submitting}
        />
        <div className="actions">
          <button type="submit" disabled={submitting || !answer.trim()}>
            {submitting ? 'Submitting...' : 'Submit Answer'}
          </button>
          {error && <span className="error-msg">{error}</span>}
        </div>
      </form>
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
