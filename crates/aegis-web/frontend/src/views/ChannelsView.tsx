import { useAppSelector } from '../store/hooks';

export function ChannelsView() {
  const channels = useAppSelector((state) => state.channels.items);
  const loading = useAppSelector((state) => state.channels.loading);

  if (loading) {
    return (
      <section className="empty-state">
        <h2>Loading channels</h2>
        <p>Fetching active channel registry.</p>
      </section>
    );
  }

  if (channels.length === 0) {
    return (
      <section className="empty-state">
        <h2>No channels</h2>
        <p>Configured channels will appear here.</p>
      </section>
    );
  }

  return (
    <section className="table-panel">
      <table>
        <thead>
          <tr>
            <th>Name</th>
            <th>Kind</th>
            <th>Status</th>
            <th>Registered</th>
          </tr>
        </thead>
        <tbody>
          {channels.map((channel) => (
            <tr key={channel.name}>
              <td>
                <strong>{channel.name}</strong>
              </td>
              <td>{channel.kind}</td>
              <td>
                <span className={channel.active ? 'channel-dot is-active' : 'channel-dot'} />
                {channel.active ? 'active' : 'inactive'}
              </td>
              <td>{formatDate(channel.registered_at)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(value));
}
