import { useEffect, useMemo, useState, type FormEvent } from 'react';
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
  DialogActions,
  Tabs,
  Tab,
  FormControl,
  RadioGroup,
  FormControlLabel,
  Radio,
  Chip,
  Alert
} from '@mui/material';
import {
  PlayArrow as ResumeIcon,
  Pause as PauseIcon,
  Stop as KillIcon,
  Launch as AttachIcon,
  Sync as FailoverIcon,
  Add as AddIcon
} from '@mui/icons-material';

import type { DesignTemplate } from '../api/rest';
import {
  failoverAgent,
  fetchAgents,
  fetchDesignTemplates,
  killAgent,
  pauseAgent,
  resumeAgent,
  spawnDesignTemplate,
  spawnTask
} from '../api/thunks';
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
  const [spawnMode, setSpawnMode] = useState<'template' | 'custom'>('template');
  const [taskPrompt, setTaskPrompt] = useState('');
  const [templates, setTemplates] = useState<DesignTemplate[]>([]);
  const [templatesLoading, setTemplatesLoading] = useState(false);
  const [templatesLoaded, setTemplatesLoaded] = useState(false);
  const [selectedTemplateName, setSelectedTemplateName] = useState('');
  const [templateVars, setTemplateVars] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const [spawnError, setSpawnError] = useState<string | null>(null);

  const selectedTemplate = useMemo(
    () => templates.find((template) => template.name === selectedTemplateName) ?? null,
    [selectedTemplateName, templates],
  );

  const templateVariableNames = useMemo(() => {
    if (!selectedTemplate) {
      return [];
    }
    const hiddenVars = new Set(['project_root']);
    return Array.from(new Set([...selectedTemplate.required, ...selectedTemplate.optional])).filter(
      (name) => !hiddenVars.has(name),
    );
  }, [selectedTemplate]);

  useEffect(() => {
    if (!modalOpen || !activeProjectId || templatesLoaded) {
      return;
    }

    setTemplatesLoading(true);
    setSpawnError(null);
    dispatch(fetchDesignTemplates(activeProjectId))
      .unwrap()
      .then((result) => {
        setTemplates(result.templates);
        setSelectedTemplateName((current) => current || result.templates[0]?.name || '');
      })
      .catch((error) => {
        const msg = error instanceof Error ? error.message : 'Unable to load templates.';
        setSpawnError(msg);
      })
      .finally(() => {
        setTemplatesLoaded(true);
        setTemplatesLoading(false);
      });
  }, [activeProjectId, dispatch, modalOpen, templatesLoaded]);

  useEffect(() => {
    setTemplates([]);
    setTemplatesLoaded(false);
    setSelectedTemplateName('');
    setTemplateVars({});
  }, [activeProjectId]);

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

    setSubmitting(true);
    setSpawnError(null);
    try {
      if (spawnMode === 'template') {
        if (!selectedTemplate) {
          setSpawnError('Select a template before spawning an agent.');
          return;
        }

        const missingVariable = selectedTemplate.required
          .filter((name) => name !== 'project_root')
          .find((name) => !templateVars[name]?.trim());
        if (missingVariable) {
          setSpawnError(`Enter ${formatVariableName(missingVariable)} before spawning an agent.`);
          return;
        }

        await dispatch(
          spawnDesignTemplate({
            projectId: activeProjectId,
            name: selectedTemplate.name,
            vars: compactVars(templateVars),
          }),
        ).unwrap();
      } else {
        if (!prompt) {
          setSpawnError('Enter a task prompt before spawning an agent.');
          return;
        }
        await dispatch(spawnTask({ projectId: activeProjectId, task: prompt })).unwrap();
      }

      toast.success('Agent spawned successfully');
      setTaskPrompt('');
      setTemplateVars({});
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
            <Tabs
              value={spawnMode}
              onChange={(_, value) => {
                setSpawnMode(value);
                setSpawnError(null);
              }}
              sx={{ mb: 2 }}
            >
              <Tab value="template" label="Template" />
              <Tab value="custom" label="Custom Prompt" />
            </Tabs>

            {spawnError && (
              <Alert severity="error" sx={{ mb: 2 }}>
                {spawnError}
              </Alert>
            )}

            {spawnMode === 'template' ? (
              <Stack spacing={2}>
                {templatesLoading ? (
                  <Box sx={{ display: 'flex', justifyContent: 'center', py: 4 }}>
                    <CircularProgress />
                  </Box>
                ) : templates.length === 0 ? (
                  <Typography variant="body2" color="text.secondary">
                    No built-in templates are available.
                  </Typography>
                ) : (
                  <FormControl fullWidth>
                    <RadioGroup
                      value={selectedTemplateName}
                      onChange={(event) => {
                        setSelectedTemplateName(event.target.value);
                        setTemplateVars({});
                        setSpawnError(null);
                      }}
                    >
                      <Stack spacing={1}>
                        {templates.map((template) => (
                          <Paper
                            key={template.name}
                            variant="outlined"
                            sx={{ p: 1.5, borderRadius: 1 }}
                          >
                            <FormControlLabel
                              value={template.name}
                              control={<Radio />}
                              label={
                                <Box>
                                  <Typography variant="subtitle2">{template.name}</Typography>
                                  <Typography variant="body2" color="text.secondary">
                                    {template.description}
                                  </Typography>
                                  <Stack direction="row" spacing={1} sx={{ mt: 1, flexWrap: 'wrap', rowGap: 1 }}>
                                    <Chip size="small" label={template.kind} />
                                    <Chip size="small" label={template.role} />
                                    <Chip size="small" label={template.provider} />
                                  </Stack>
                                </Box>
                              }
                              sx={{ alignItems: 'flex-start', m: 0, width: '100%' }}
                            />
                          </Paper>
                        ))}
                      </Stack>
                    </RadioGroup>
                  </FormControl>
                )}

                {selectedTemplate && templateVariableNames.length > 0 && (
                  <Stack spacing={2}>
                    {templateVariableNames.map((name) => (
                      <TextField
                        key={name}
                        fullWidth
                        multiline={isLongVariable(name)}
                        rows={isLongVariable(name) ? 3 : 1}
                        label={formatVariableName(name)}
                        value={templateVars[name] ?? ''}
                        onChange={(event) =>
                          setTemplateVars((current) => ({
                            ...current,
                            [name]: event.target.value,
                          }))
                        }
                        required={selectedTemplate.required.includes(name)}
                        disabled={submitting}
                      />
                    ))}
                  </Stack>
                )}
              </Stack>
            ) : (
              <TextField
                fullWidth
                multiline
                rows={4}
                variant="outlined"
                placeholder="Describe the task for the new agent..."
                value={taskPrompt}
                onChange={(e) => setTaskPrompt(e.target.value)}
                disabled={submitting}
                autoFocus
              />
            )}
          </DialogContent>
          <DialogActions sx={{ px: 3, pb: 2 }}>
            <Button onClick={() => setModalOpen(false)} disabled={submitting}>
              Cancel
            </Button>
            <Button
              type="submit"
              variant="contained"
              disabled={
                submitting ||
                (spawnMode === 'custom' && taskPrompt.trim().length === 0) ||
                (spawnMode === 'template' && (!selectedTemplate || templatesLoading))
              }
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
                        aria-label="Spawn New Agent"
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

function compactVars(vars: Record<string, string>) {
  return Object.fromEntries(
    Object.entries(vars)
      .map(([key, value]) => [key, value.trim()])
      .filter(([, value]) => value.length > 0),
  );
}

function formatVariableName(name: string) {
  return name
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function isLongVariable(name: string) {
  return name.includes('description') || name.endsWith('path');
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
