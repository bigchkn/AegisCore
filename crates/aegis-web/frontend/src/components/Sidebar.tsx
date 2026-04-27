import { NavLink } from 'react-router-dom';
import { useLocation } from 'react-router-dom';
import { useAppSelector } from '../store/hooks';
import type { ActiveView } from '../store/domain';

const navItems: Array<{ id: ActiveView; label: string }> = [
  { id: 'agents', label: 'Agents' },
  { id: 'pane', label: 'Pane' },
  { id: 'logs', label: 'Logs' },
  { id: 'tasks', label: 'Tasks' },
  { id: 'channels', label: 'Channels' },
  { id: 'taskflow', label: 'Taskflow' },
  { id: 'clarifications', label: 'Clarifications' },
];

export function Sidebar() {
  const location = useLocation();
  const projects = useAppSelector((state) => state.projects.items);
  const projectsLoading = useAppSelector((state) => state.projects.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const currentAgentId = agentIdFromPath(location.pathname);
  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark">A</span>
        <span>AegisCore</span>
      </div>

      <section className="sidebar-section">
        <h2>Projects</h2>
        <div className="project-list">
          {projectsLoading ? <span className="muted">Loading projects</span> : null}
          {projects.map((project) => (
            <NavLink
              key={project.id}
              to={`/projects/${project.id}`}
              className={({ isActive }) => 
                isActive || project.id === activeProjectId ? 'nav-button is-active' : 'nav-button'
              }
            >
              <span>{projectName(project.root_path)}</span>
            </NavLink>
          ))}
          {!projectsLoading && projects.length === 0 ? (
            <span className="muted">No registered projects</span>
          ) : null}
        </div>
      </section>

      <nav className="sidebar-section">
        <h2>Views</h2>
        {navItems.map((item) => (
          <NavLink
            key={item.id}
            to={viewRoute(activeProjectId, item.id, currentAgentId)}
            className={({ isActive }) => isActive ? 'nav-button is-active' : 'nav-button'}
          >
            <span>{item.label}</span>
          </NavLink>
        ))}
      </nav>
    </aside>
  );
}

function projectName(path: string) {
  const parts = path.split('/').filter(Boolean);
  return parts.at(-1) ?? path;
}

function agentIdFromPath(pathname: string) {
  const parts = pathname.split('/').filter(Boolean);
  if (parts.length >= 4 && parts[0] === 'projects' && (parts[2] === 'pane' || parts[2] === 'logs')) {
    return parts[3] ?? null;
  }

  if (parts.length >= 2 && (parts[0] === 'pane' || parts[0] === 'logs')) {
    return parts[1] ?? null;
  }

  return null;
}

function viewRoute(projectId: string | null, view: ActiveView, agentId: string | null) {
  const base = projectId ? `/projects/${projectId}/${view}` : `/${view}`;
  if ((view === 'pane' || view === 'logs') && agentId) {
    return `${base}/${agentId}`;
  }

  return base;
}
