import { useEffect } from 'react';

import { fetchProjectData, fetchProjects } from '../api/thunks';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { AgentsView } from '../views/AgentsView';
import { Sidebar } from './Sidebar';

export function App() {
  const dispatch = useAppDispatch();
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const connectionState = useAppSelector((state) => state.ui.connectionState);
  const error = useAppSelector((state) => state.ui.error);

  useEffect(() => {
    void dispatch(fetchProjects());
  }, [dispatch]);

  useEffect(() => {
    if (activeProjectId) {
      void dispatch(fetchProjectData(activeProjectId));
    }
  }, [activeProjectId, dispatch]);

  return (
    <main className="app-shell">
      <Sidebar />
      <section className="workspace">
        <header className="topbar">
          <div>
            <h1>Agents</h1>
            <p>Live sessions, provider state, and direct controls.</p>
          </div>
          <span className="connection-pill">{connectionState}</span>
        </header>
        {error ? <div className="banner">{error}</div> : null}
        <AgentsView />
      </section>
    </main>
  );
}
