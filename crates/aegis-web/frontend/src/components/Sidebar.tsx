import { NavLink, useLocation } from 'react-router-dom';
import { 
  Drawer, 
  List, 
  ListItem, 
  ListItemIcon, 
  ListItemText, 
  ListItemButton,
  Typography, 
  Divider, 
  Box,
  Tooltip
} from '@mui/material';
import {
  SmartToy as AgentIcon,
  Terminal as PaneIcon,
  ReceiptLong as LogIcon,
  Assignment as TaskIcon,
  RssFeed as ChannelIcon,
  Timeline as TaskflowIcon,
  HelpOutlined as ClarificationIcon,
  Folder as ProjectIcon,
  ChevronLeft as CollapseIcon,
  Menu as ExpandIcon
} from '@mui/icons-material';

import { agentIdFromLocation, agentRoute } from '../lib/agentRoutes';
import { useAppDispatch, useAppSelector } from '../store/hooks';
import { toggleSidebar } from '../store/uiSlice';
import type { ActiveView } from '../store/domain';

const DRAWER_WIDTH = 240;

const navItems: Array<{ id: ActiveView; label: string; icon: React.ReactNode }> = [
  { id: 'agents', label: 'Agents', icon: <AgentIcon /> },
  { id: 'pane', label: 'Pane', icon: <PaneIcon /> },
  { id: 'logs', label: 'Logs', icon: <LogIcon /> },
  { id: 'tasks', label: 'Tasks', icon: <TaskIcon /> },
  { id: 'channels', label: 'Channels', icon: <ChannelIcon /> },
  { id: 'taskflow', label: 'Taskflow', icon: <TaskflowIcon /> },
  { id: 'clarifications', label: 'Clarifications', icon: <ClarificationIcon /> },
];

export function Sidebar() {
  const location = useLocation();
  const dispatch = useAppDispatch();
  const projects = useAppSelector((state) => state.projects.items);
  const projectsLoading = useAppSelector((state) => state.projects.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  const open = useAppSelector((state) => state.ui.sidebarOpen);
  const currentAgentId = agentIdFromLocation(location.pathname, location.search);

  return (
    <Drawer
      variant="permanent"
      sx={{
        width: open ? DRAWER_WIDTH : 64,
        flexShrink: 0,
        '& .MuiDrawer-paper': {
          width: open ? DRAWER_WIDTH : 64,
          boxSizing: 'border-box',
          transition: (theme) => theme.transitions.create('width', {
            easing: theme.transitions.easing.sharp,
            duration: theme.transitions.duration.enteringScreen,
          }),
          overflowX: 'hidden',
          backgroundColor: 'background.paper',
          borderRight: '1px solid',
          borderColor: 'divider',
        },
      }}
      open={open}
    >
      <Box sx={{ 
        display: 'flex', 
        alignItems: 'center', 
        justifyContent: open ? 'space-between' : 'center',
        px: open ? 2 : 0,
        py: 2,
        minHeight: 64 
      }}>
        {open && (
          <Typography variant="h6" color="primary" sx={{ fontWeight: 'bold', letterSpacing: 1 }}>
            AEGIS
          </Typography>
        )}
        <ListItemButton 
          onClick={() => dispatch(toggleSidebar())}
          sx={{ 
            minWidth: 0, 
            p: 1,
            justifyContent: 'center',
            borderRadius: 1
          }}
        >
          {open ? <CollapseIcon fontSize="small" /> : <ExpandIcon fontSize="small" />}
        </ListItemButton>
      </Box>

      <Divider />

      <List sx={{ px: 1 }}>
        <Typography 
          variant="overline" 
          sx={{ 
            px: 2, 
            display: open ? 'block' : 'none',
            color: 'text.secondary',
            fontWeight: 'bold'
          }}
        >
          Views
        </Typography>
        {navItems.map((item) => {
          const isActive = location.pathname.includes(`/${item.id}`);
          const to = agentRoute(activeProjectId, item.id, currentAgentId);
          
          return (
            <Tooltip key={item.id} title={!open ? item.label : ''} placement="right">
              <ListItem disablePadding sx={{ display: 'block', mb: 0.5 }}>
                <ListItemButton
                  component={NavLink}
                  to={to}
                  sx={{
                    minHeight: 48,
                    justifyContent: open ? 'initial' : 'center',
                    px: 2.5,
                    borderRadius: 2,
                    backgroundColor: isActive ? 'primary.main' : 'transparent',
                    color: isActive ? 'primary.contrastText' : 'text.primary',
                    '&:hover': {
                      backgroundColor: isActive ? 'primary.dark' : 'rgba(255, 255, 255, 0.08)',
                    },
                  }}
                >
                  <ListItemIcon
                    sx={{
                      minWidth: 0,
                      mr: open ? 2 : 'auto',
                      justifyContent: 'center',
                      color: 'inherit'
                    }}
                  >
                    {item.icon}
                  </ListItemIcon>
                  <ListItemText 
                    sx={{ opacity: open ? 1 : 0 }}
                    primary={
                      <Typography variant="body2" sx={{ fontWeight: isActive ? 600 : 400, fontSize: '0.9rem' }}>
                        {item.label}
                      </Typography>
                    }
                  />
                </ListItemButton>
              </ListItem>
            </Tooltip>
          );
        })}
      </List>

      <Divider sx={{ my: 1 }} />

      <List sx={{ px: 1 }}>
        <Typography 
          variant="overline" 
          sx={{ 
            px: 2, 
            display: open ? 'block' : 'none',
            color: 'text.secondary',
            fontWeight: 'bold'
          }}
        >
          Projects
        </Typography>
        {projectsLoading && open && (
          <ListItem sx={{ px: 2 }}>
            <Typography variant="caption" color="text.secondary">Loading...</Typography>
          </ListItem>
        )}
        {projects.map((project) => {
          const isProjectActive = project.id === activeProjectId;
          const name = projectName(project.root_path);
          return (
            <Tooltip key={project.id} title={!open ? name : ''} placement="right">
              <ListItem disablePadding sx={{ display: 'block', mb: 0.5 }}>
                <ListItemButton
                  component={NavLink}
                  to={`/projects/${project.id}`}
                  sx={{
                    minHeight: 48,
                    justifyContent: open ? 'initial' : 'center',
                    px: 2.5,
                    borderRadius: 2,
                    borderLeft: isProjectActive ? '3px solid' : '3px solid transparent',
                    borderColor: 'primary.main',
                    backgroundColor: isProjectActive ? 'rgba(156, 39, 176, 0.08)' : 'transparent',
                  }}
                >
                  <ListItemIcon
                    sx={{
                      minWidth: 0,
                      mr: open ? 2 : 'auto',
                      justifyContent: 'center',
                      color: isProjectActive ? 'primary.main' : 'inherit'
                    }}
                  >
                    <ProjectIcon />
                  </ListItemIcon>
                  <ListItemText 
                    sx={{ opacity: open ? 1 : 0 }}
                    primary={
                      <Typography variant="body2" noWrap sx={{ fontWeight: isProjectActive ? 600 : 400, fontSize: '0.85rem' }}>
                        {name}
                      </Typography>
                    }
                  />
                </ListItemButton>
              </ListItem>
            </Tooltip>
          );
        })}
      </List>
    </Drawer>
  );
}

function projectName(path: string) {
  const parts = path.split('/').filter(Boolean);
  return parts.at(-1) ?? path;
}
