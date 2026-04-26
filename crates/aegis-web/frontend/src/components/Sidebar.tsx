import { NavLink } from 'react-router-dom';
import { useAppSelector } from '../store/hooks';
import type { ActiveView } from '../store/domain';

const navItems: Array<{ id: ActiveView; label: string }> = [
  { id: 'agents', label: 'Agents' },
  { id: 'pane', label: 'Pane' },
  { id: 'logs', label: 'Logs' },
  { id: 'tasks', label: 'Tasks' },
  { id: 'channels', label: 'Channels' },
  { id: 'taskflow', label: 'Taskflow' },
];

export function Sidebar() {
  const projects = useAppSelector((state) => state.projects.items);
  const projectsLoading = useAppSelector((state) => state.projects.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const activeView = useAppSelector((state) => {
    // We still use the location in App, but for sidebar highlighting 
    // it's easier to keep the pattern consistent if we had one.
    // For now, we'll let NavLink handle its own active state.
    return state.ui.activeView;
  });

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
              to={`/projects/${project.id}/agents`}
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
            to={activeProjectId ? `/projects/${activeProjectId}/${item.id}` : `/${item.id}`}
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
