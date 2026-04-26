import { useEffect } from 'react';
import { Routes, Route, Navigate, useLocation, useParams } from 'react-router-dom';

import { fetchProjectData, fetchProjects } from '../api/thunks';
import { setActiveProject, setSelectedAgent } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { AgentsView } from '../views/AgentsView';
import { ChannelsView } from '../views/ChannelsView';
import { LogView } from '../views/LogView';
import { PaneView } from '../views/PaneView';
import { TaskflowView } from '../views/TaskflowView';
import { TasksView } from '../views/TasksView';
import { ClarificationsView } from '../views/ClarificationsView';
import { Sidebar } from './Sidebar';

export function App() {
  const dispatch = useAppDispatch();
  const location = useLocation();
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const connectionState = useAppSelector((state) => state.ui.connectionState);
  const error = useAppSelector((state) => state.ui.error);
  const projects = useAppSelector((state) => state.projects.items);
  const activeProject = projects.find((project) => project.id === activeProjectId);

  // Derive active view from path for the title
  const pathParts = location.pathname.split('/').filter(Boolean);
  // /projects/:id/:view -> parts are ["projects", ":id", ":view"]
  // /:view -> parts are [":view"]
  const activeViewPath = pathParts.includes('projects') ? pathParts[2] : pathParts[0] || 'agents';

  useEffect(() => {
    void dispatch(fetchProjects());
  }, [dispatch]);

  useEffect(() => {
    if (activeProjectId) {
      void dispatch(fetchProjectData(activeProjectId));
    }
  }, [activeProjectId, dispatch]);

  useEffect(() => {
    if (!activeProjectId && projects.length > 0 && !location.pathname.includes('/projects/')) {
      dispatch(setActiveProject(projects[0].id));
    }
  }, [activeProjectId, dispatch, projects, location.pathname]);

  return (
    <main className="app-shell">
      <Sidebar />
      <section className="workspace">
        <header className="topbar">
          <div>
            <h1>{titleForView(activeViewPath)}</h1>
            <p>{activeProject ? activeProject.root_path : 'No active project'}</p>
          </div>
          <span className="connection-pill">{connectionState}</span>
        </header>
        {error ? <div className="banner">{error}</div> : null}
        
        <Routes>
          <Route path="/" element={<ProjectRedirect projects={projects} />} />
          <Route path="/projects/:projectId/*" element={<ProjectRoutes />} />
          
          {/* Legacy/Direct routes */}
          <Route path="/agents" element={<AgentsView />} />
          <Route path="/pane" element={<PaneView />} />
          <Route path="/logs" element={<LogView />} />
          <Route path="/tasks" element={<TasksView />} />
          <Route path="/channels" element={<ChannelsView />} />
          <Route path="/taskflow" element={<TaskflowView />} />
          <Route path="/clarifications" element={<ClarificationsView />} />
          
          <Route path="*" element={<Navigate to="/agents" replace />} />
        </Routes>
      </section>
    </main>
  );
}

function ProjectRedirect({ projects }: { projects: any[] }) {
  if (projects.length > 0) {
    return <Navigate to={`/projects/${projects[0].id}/agents`} replace />;
  }
  return <Navigate to="/agents" replace />;
}

function ProjectRoutes() {
  const { projectId } = useParams();
  const dispatch = useAppDispatch();
  const location = useLocation();
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const projects = useAppSelector((state) => state.projects.items);
  const activeProject = projects.find((p) => p.id === projectId);

  useEffect(() => {
    if (projectId && projectId !== activeProjectId) {
      dispatch(setActiveProject(projectId));
    }
  }, [projectId, activeProjectId, dispatch]);

  // Handle auto-attach redirect on initial project load
  if (
    activeProject?.last_attached_agent_id &&
    (location.pathname === `/projects/${projectId}` || location.pathname === `/projects/${projectId}/` || location.pathname === `/projects/${projectId}/agents`)
  ) {
    // We only want to do this once on initial landing, but simple redirect is usually fine
    // if the user hasn't explicitly navigated elsewhere. 
    // For now, if they land on agents list and have a last_attached, we send them to pane.
    if (location.pathname.endsWith('/agents') || location.pathname === `/projects/${projectId}` || location.pathname === `/projects/${projectId}/`) {
        dispatch(setSelectedAgent(activeProject.last_attached_agent_id));
        return <Navigate to={`/projects/${projectId}/pane`} replace />;
    }
  }

  return (
    <Routes>
      <Route path="agents" element={<AgentsView />} />
      <Route path="pane" element={<PaneView />} />
      <Route path="logs" element={<LogView />} />
      <Route path="tasks" element={<TasksView />} />
      <Route path="channels" element={<ChannelsView />} />
      <Route path="taskflow" element={<TaskflowView />} />
      <Route path="clarifications" element={<ClarificationsView />} />
      <Route path="*" element={<Navigate to="agents" replace />} />
    </Routes>
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
    case 'clarifications':
      return 'Clarifications';
    case 'agents':
    default:
      return 'Agents';
  }
}
