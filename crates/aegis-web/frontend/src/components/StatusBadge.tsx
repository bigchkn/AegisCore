import { Chip } from '@mui/material';
import type { AgentStatus } from '../types/AgentStatus';

export function StatusBadge({ status }: { status: AgentStatus }) {
  let color: 'default' | 'primary' | 'secondary' | 'error' | 'info' | 'success' | 'warning' = 'default';
  
  switch (status) {
    case 'active':
      color = 'success';
      break;
    case 'paused':
      color = 'warning';
      break;
    case 'failed':
      color = 'error';
      break;
    case 'terminated':
      color = 'info';
      break;
    case 'queued':
    case 'starting':
      color = 'default';
      break;
    case 'cooling':
    case 'reporting':
      color = 'secondary';
      break;
    default:
      color = 'default';
  }

  return (
    <Chip 
      label={status} 
      color={color} 
      size="small" 
      variant="outlined"
      sx={{ 
        fontWeight: 'bold', 
        textTransform: 'uppercase', 
        fontSize: '0.65rem',
        borderRadius: 1
      }} 
    />
  );
}
