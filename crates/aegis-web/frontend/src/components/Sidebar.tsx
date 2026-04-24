import { fetchProjectData } from '../api/thunks';
import { setActiveProject, setActiveView } from '../store/uiSlice';
import { useAppDispatch, useAppSelector } from '../store/hooks';
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
  const dispatch = useAppDispatch();
  const projects = useAppSelector((state) => state.projects.items);
  const projectsLoading = useAppSelector((state) => state.projects.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const activeView = useAppSelector((state) => state.ui.activeView);

  const selectProject = (projectId: string) => {
    dispatch(setActiveProject(projectId));
    void dispatch(fetchProjectData(projectId));
  };

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
            <button
              key={project.id}
              type="button"
              className={project.id === activeProjectId ? 'nav-button is-active' : 'nav-button'}
              onClick={() => selectProject(project.id)}
            >
              <span>{projectName(project.root_path)}</span>
            </button>
          ))}
          {!projectsLoading && projects.length === 0 ? (
            <span className="muted">No registered projects</span>
          ) : null}
        </div>
      </section>

      <nav className="sidebar-section">
        <h2>Views</h2>
        {navItems.map((item) => (
          <button
            key={item.id}
            type="button"
            className={item.id === activeView ? 'nav-button is-active' : 'nav-button'}
            onClick={() => dispatch(setActiveView(item.id))}
          >
            <span>{item.label}</span>
          </button>
        ))}
      </nav>
    </aside>
  );
}

function projectName(path: string) {
  const parts = path.split('/').filter(Boolean);
  return parts.at(-1) ?? path;
}
