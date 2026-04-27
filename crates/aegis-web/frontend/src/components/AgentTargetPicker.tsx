import type { Agent } from '../types/Agent';

type AgentTargetPickerProps = {
  agents: Agent[];
  selectedAgentId: string | null;
  label: string;
  onSelect: (agentId: string | null) => void;
};

export function AgentTargetPicker({ agents, selectedAgentId, label, onSelect }: AgentTargetPickerProps) {
  return (
    <label className="agent-target-picker">
      <span>{label}</span>
      <select
        value={selectedAgentId ?? ''}
        onChange={(event) => onSelect(event.target.value || null)}
        disabled={agents.length === 0}
      >
        <option value="">{agents.length === 0 ? 'No agents available' : 'Select agent'}</option>
        {agents.map((agent) => (
          <option key={agent.agent_id} value={agent.agent_id}>
            {agent.name} ({agent.agent_id.slice(0, 8)})
          </option>
        ))}
      </select>
    </label>
  );
}
