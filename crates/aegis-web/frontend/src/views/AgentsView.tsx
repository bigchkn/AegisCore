import { useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { toast } from 'sonner';
import {
  Box,
  Typography,
  TextField,
  Button,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Paper,
  Stack,
  IconButton,
  Tooltip,
  CircularProgress,
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions
} from '@mui/material';
import {
  PlayArrow as ResumeIcon,
  Pause as PauseIcon,
  Stop as KillIcon,
  Launch as AttachIcon,
  Sync as FailoverIcon,
  Add as AddIcon
} from '@mui/icons-material';

import { failoverAgent, killAgent, pauseAgent, resumeAgent, spawnTask, fetchAgents } from '../api/thunks';
import { StatusBadge } from '../components/StatusBadge';
import { agentRoute } from '../lib/agentRoutes';
import { useAppDispatch, useAppSelector } from '../store/hooks';

export function AgentsView() {
  const dispatch = useAppDispatch();
  const navigate = useNavigate();
  const agents = useAppSelector((state) => state.agents.items);
  const loading = useAppSelector((state) => state.agents.loading);
  const activeProjectId = useAppSelector((state) => state.ui.activeProjectId);
  
  const [modalOpen, setModalOpen] = useState(false);
  const [taskPrompt, setTaskPrompt] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [spawnError, setSpawnError] = useState<string | null>(null);

  function attachAgent(agentId: string) {
    if (!activeProjectId) {
      return;
    }
    navigate(agentRoute(activeProjectId, 'pane', agentId));
  }

  async function handleSpawn(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!activeProjectId) {
      return;
    }

    const prompt = taskPrompt.trim();
    if (!prompt) {
      setSpawnError('Enter a task prompt before spawning an agent.');
      return;
    }

    setSubmitting(true);
    setSpawnError(null);
    try {
      await dispatch(spawnTask({ projectId: activeProjectId, task: prompt })).unwrap();
      toast.success('Agent spawned successfully');
      setTaskPrompt('');
      setModalOpen(false);
      // Refresh agents list to show the new agent
      dispatch(fetchAgents(activeProjectId));
    } catch (error) {
      const msg = error instanceof Error ? error.message : 'Unable to spawn agent.';
      setSpawnError(msg);
      toast.error('Spawn failed', { description: msg });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleAction(action: any, agentId: string, label: string) {
    if (!activeProjectId) return;
    try {
      await dispatch(action({ projectId: activeProjectId, agentId })).unwrap();
      toast.success(`${label} successful`);
    } catch (error) {
      const msg = error instanceof Error ? error.message : `Failed to ${label.toLowerCase()}`;
      toast.error(`${label} failed`, { description: msg });
    }
  }

  return (
    <Stack spacing={3}>
      <Dialog 
        open={modalOpen} 
        onClose={() => !submitting && setModalOpen(false)}
        maxWidth="sm"
        fullWidth
      >
        <Box component="form" onSubmit={handleSpawn}>
          <DialogTitle>Spawn Agent</DialogTitle>
          <DialogContent>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
              Submit a task prompt to create a new agent session.
            </Typography>
            <TextField
              fullWidth
              multiline
              rows={4}
              variant="outlined"
              placeholder="Describe the task for the new agent..."
              value={taskPrompt}
              onChange={(e) => setTaskPrompt(e.target.value)}
              disabled={submitting}
              error={!!spawnError}
              helperText={spawnError}
              autoFocus
            />
          </DialogContent>
          <DialogActions sx={{ px: 3, pb: 2 }}>
            <Button onClick={() => setModalOpen(false)} disabled={submitting}>
              Cancel
            </Button>
            <Button
              type="submit"
              variant="contained"
              disabled={submitting || taskPrompt.trim().length === 0}
              startIcon={submitting && <CircularProgress size={20} color="inherit" />}
            >
              {submitting ? 'Spawning...' : 'Spawn'}
            </Button>
          </DialogActions>
        </Box>
      </Dialog>

      {!activeProjectId ? (
        <EmptyPanel title="Select a project" body="Registered projects appear in the sidebar." />
      ) : loading && agents.length === 0 ? (
        <Box sx={{ display: 'flex', justifyContent: 'center', p: 4 }}>
          <CircularProgress />
        </Box>
      ) : (
        <TableContainer component={Paper} elevation={0} sx={{ border: '1px solid', borderColor: 'divider' }}>
          <Table sx={{ minWidth: 650 }}>
            <TableHead>
              <TableRow sx={{ bgcolor: 'action.hover' }}>
                <TableCell colSpan={6}>
                  <Stack direction="row" sx={{ alignItems: 'center', justifyContent: 'space-between' }}>
                    <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
                      Active Agents ({agents.length})
                    </Typography>
                    <Tooltip title="Spawn New Agent">
                      <IconButton 
                        size="small" 
                        color="primary"
                        onClick={() => setModalOpen(true)}
                        sx={{ border: '1px solid', borderColor: 'primary.main' }}
                      >
                        <AddIcon fontSize="small" />
                      </IconButton>
                    </Tooltip>
                  </Stack>
                </TableCell>
              </TableRow>
              <TableRow>
                <TableCell>Name</TableCell>
                <TableCell>Kind</TableCell>
                <TableCell>Status</TableCell>
                <TableCell>Provider</TableCell>
                <TableCell>Task</TableCell>
                <TableCell align="right">Actions</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {agents.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={6} align="center" sx={{ py: 8 }}>
                    <Typography variant="body2" color="text.secondary">
                      No active agents. Click "Spawn Agent" to start one.
                    </Typography>
                  </TableCell>
                </TableRow>
              ) : (
                agents.map((agent) => (
                  <TableRow
                    key={agent.agent_id}
                    hover
                    onClick={() => attachAgent(agent.agent_id)}
                    sx={{ cursor: 'pointer', '&:last-child td, &:last-child th': { border: 0 } }}
                  >
                    <TableCell>
                      <Typography variant="body2" sx={{ fontWeight: 600 }}>{agent.name}</Typography>
                      <Typography variant="caption" color="text.secondary">{agent.role}</Typography>
                    </TableCell>
                    <TableCell>{agent.kind}</TableCell>
                    <TableCell>
                      <StatusBadge status={agent.status} />
                    </TableCell>
                    <TableCell>{agent.cli_provider}</TableCell>
                    <TableCell>{agent.task_id ?? 'none'}</TableCell>
                    <TableCell align="right" onClick={(e) => e.stopPropagation()}>
                      <Stack direction="row" spacing={1} sx={{ justifyContent: 'flex-end' }}>
                        <Tooltip title="Attach">
                          <IconButton size="small" onClick={() => attachAgent(agent.agent_id)} color="primary">
                            <AttachIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title="Pause">
                          <IconButton size="small" onClick={() => handleAction(pauseAgent, agent.agent_id, 'Pause')}>
                            <PauseIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title="Resume">
                          <IconButton size="small" onClick={() => handleAction(resumeAgent, agent.agent_id, 'Resume')}>
                            <ResumeIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title="Failover">
                          <IconButton size="small" onClick={() => handleAction(failoverAgent, agent.agent_id, 'Failover')}>
                            <FailoverIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                        <Tooltip title="Kill">
                          <IconButton size="small" onClick={() => handleAction(killAgent, agent.agent_id, 'Kill')} color="error">
                            <KillIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                      </Stack>
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </TableContainer>
      )}
    </Stack>
  );
}

function EmptyPanel({ title, body }: { title: string; body: string }) {
  return (
    <Paper 
      variant="outlined" 
      sx={{ 
        p: 6, 
        textAlign: 'center', 
        bgcolor: 'background.paper',
        borderStyle: 'dashed',
        borderWidth: 2
      }}
    >
      <Typography variant="h6" color="text.secondary" gutterBottom>
        {title}
      </Typography>
      <Typography variant="body2" color="text.secondary">
        {body}
      </Typography>
    </Paper>
  );
}
