import React from 'react';
import ReactDOM from 'react-dom/client';

import './styles.css';

function App() {
  return (
    <main className="app-shell">
      <section className="empty-state">
        <h1>AegisCore</h1>
        <p>Web control plane is starting up.</p>
      </section>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
