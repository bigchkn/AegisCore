import { useEffect } from 'react';
import { Routes, Route, Navigate, useLocation, useParams } from 'react-router-dom';
import { Toaster } from 'sonner';
import { 
  Box, 
  AppBar, 
  Toolbar, 
  Typography, 
  Chip,
  Container,
  Paper
} from '@mui/material';

import { fetchProjectData, fetchProjects } from '../api/thunks';
import { setActiveProject, toggleSidebar } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { agentRoute } from '../lib/agentRoutes';
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
  const sidebarOpen = useAppSelector((state) => state.ui.sidebarOpen);
  const error = useAppSelector((state) => state.ui.error);
  const projects = useAppSelector((state) => state.projects.items);
  const activeProject = projects.find((project) => project.id === activeProjectId);

  // Derive active view from path for the title
  const pathParts = location.pathname.split('/').filter(Boolean);
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
    if (!activeProjectId) return;
    const id = setInterval(() => {
      void dispatch(fetchProjectData(activeProjectId));
    }, 5_000);
    return () => clearInterval(id);
  }, [activeProjectId, dispatch]);

  useEffect(() => {
    if (!activeProjectId && projects.length > 0 && !location.pathname.includes('/projects/')) {
      dispatch(setActiveProject(projects[0].id));
    }
  }, [activeProjectId, dispatch, projects, location.pathname]);

  return (
    <Box sx={{ display: 'flex', minHeight: '100vh', bgcolor: 'background.default' }}>
      <Toaster position="bottom-right" richColors />
      <Sidebar />
      
      <Box
        component="main"
        sx={{
          flexGrow: 1,
          display: 'flex',
          flexDirection: 'column',
          width: { sm: `calc(100% - ${sidebarOpen ? 240 : 64}px)` },
          transition: (theme) => theme.transitions.create(['width', 'margin'], {
            easing: theme.transitions.easing.sharp,
            duration: theme.transitions.duration.leavingScreen,
          }),
        }}
      >
        <AppBar 
          position="sticky" 
          elevation={0}
          sx={{ 
            backgroundColor: 'background.paper',
            borderBottom: '1px solid',
            borderColor: 'divider',
            color: 'text.primary'
          }}
        >
          <Toolbar>
            <Box sx={{ flexGrow: 1 }}>
              <Typography variant="h6" noWrap component="div" sx={{ fontWeight: 600 }}>
                {titleForView(activeViewPath)}
              </Typography>
              <Typography variant="caption" sx={{ color: 'text.secondary', display: 'block', mt: -0.5 }}>
                {activeProject ? activeProject.root_path : 'No active project'}
              </Typography>
            </Box>

            <Chip 
              label={connectionState} 
              size="small" 
              color={connectionState === 'connected' ? 'success' : 'warning'}
              variant="outlined"
              sx={{ fontWeight: 'bold', textTransform: 'uppercase', fontSize: '0.65rem' }}
            />
          </Toolbar>
        </AppBar>

        {error && (
          <Paper 
            square 
            sx={{ 
              p: 1.5, 
              bgcolor: 'error.main', 
              color: 'error.contrastText',
              textAlign: 'center'
            }}
          >
            <Typography variant="body2" sx={{ fontWeight: 500 }}>{error}</Typography>
          </Paper>
        )}

        <Container maxWidth={false} sx={{ mt: 3, mb: 3, flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
          <Routes>
            <Route path="/" element={<ProjectRedirect projects={projects} />} />
            <Route path="/projects/:projectId/*" element={<ProjectRoutes />} />
            
            <Route path="/agents" element={<AgentsView />} />
            <Route path="/pane/:agentId?" element={<PaneView />} />
            <Route path="/logs/:agentId?" element={<LogView />} />
            <Route path="/tasks" element={<TasksView />} />
            <Route path="/channels" element={<ChannelsView />} />
            <Route path="/taskflow" element={<TaskflowView />} />
            <Route path="/clarifications" element={<ClarificationsView />} />
            
            <Route path="*" element={<Navigate to="/agents" replace />} />
          </Routes>
        </Container>
      </Box>
    </Box>
  );
}

function ProjectRedirect({ projects }: { projects: any[] }) {
  if (projects.length > 0) {
    return <Navigate to={`/projects/${projects[0].id}`} replace />;
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

  if (
    activeProject?.last_attached_agent_id &&
    (location.pathname === `/projects/${projectId}` || location.pathname === `/projects/${projectId}/`)
  ) {
    return <Navigate to={agentRoute(projectId ?? null, 'pane', activeProject.last_attached_agent_id)} replace />;
  }

  return (
    <Routes>
      <Route path="agents" element={<AgentsView />} />
      <Route path="pane/:agentId?" element={<PaneView />} />
      <Route path="logs/:agentId?" element={<LogView />} />
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
