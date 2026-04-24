import { useAppSelector } from '../store/hooks';

export function App() {
  const connectionState = useAppSelector((state) => state.ui.connectionState);

  return (
    <main className="app-shell">
      <section className="empty-state">
        <h1>AegisCore</h1>
        <p>Web control plane is starting up.</p>
        <span className="connection-pill">{connectionState}</span>
      </section>
    </main>
  );
}
