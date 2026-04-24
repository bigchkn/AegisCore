import { useEffect } from 'react';

import { fetchProjectData, fetchProjects } from '../api/thunks';
import { setActiveProject } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { AgentsView } from '../views/AgentsView';
import { ChannelsView } from '../views/ChannelsView';
import { LogView } from '../views/LogView';
import { PaneView } from '../views/PaneView';
import { TaskflowView } from '../views/TaskflowView';
import { TasksView } from '../views/TasksView';
import { Sidebar } from './Sidebar';

export function App() {
  const dispatch = useAppDispatch();
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const activeView = useAppSelector((state) => state.ui.activeView);
  const connectionState = useAppSelector((state) => state.ui.connectionState);
  const error = useAppSelector((state) => state.ui.error);
  const projects = useAppSelector((state) => state.projects.items);
  const activeProject = projects.find((project) => project.id === activeProjectId);

  useEffect(() => {
    void dispatch(fetchProjects());
  }, [dispatch]);

  useEffect(() => {
    if (activeProjectId) {
      void dispatch(fetchProjectData(activeProjectId));
    }
  }, [activeProjectId, dispatch]);

  useEffect(() => {
    if (!activeProjectId && projects.length > 0) {
      dispatch(setActiveProject(projects[0].id));
    }
  }, [activeProjectId, dispatch, projects]);

  return (
    <main className="app-shell">
      <Sidebar />
      <section className="workspace">
        <header className="topbar">
          <div>
            <h1>{titleForView(activeView)}</h1>
            <p>{activeProject ? activeProject.root_path : 'No active project'}</p>
          </div>
          <span className="connection-pill">{connectionState}</span>
        </header>
        {error ? <div className="banner">{error}</div> : null}
        {activeView === 'pane' ? <PaneView /> : null}
        {activeView === 'logs' ? <LogView /> : null}
        {activeView === 'tasks' ? <TasksView /> : null}
        {activeView === 'channels' ? <ChannelsView /> : null}
        {activeView === 'taskflow' ? <TaskflowView /> : null}
        {activeView === 'agents' ? <AgentsView /> : null}
      </section>
    </main>
  );
}

function titleForView(view: string) {
  switch (view) {
    case 'pane':
      return 'Pane';
    case 'logs':
      return 'Logs';
    case 'tasks':
      return 'Tasks';
    case 'channels':
      return 'Channels';
    case 'taskflow':
      return 'Taskflow';
    default:
      return 'Agents';
  }
}
