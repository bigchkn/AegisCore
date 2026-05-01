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
  Article as DesignsIcon,
  HelpOutlined as ClarificationIcon,
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
  { id: 'designs', label: 'Designs', icon: <DesignsIcon /> },
  { id: 'clarifications', label: 'Clarifications', icon: <ClarificationIcon /> },
];

export function Sidebar() {
  const location = useLocation();
  const dispatch = useAppDispatch();
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

    </Drawer>
  );
}
