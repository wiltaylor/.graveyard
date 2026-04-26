/* global React, Sidebar, TopBar, Dashboard, Services, Logs, Settings */
const { useState, useEffect } = React;

const ROUTE_TITLES = {
  dashboard: ['forge', 'Dashboard'],
  services:  ['forge', 'Services'],
  logs:      ['forge', 'Logs'],
  metrics:   ['forge', 'Metrics'],
  database:  ['forge', 'Database'],
  environments: ['forge', 'Environments'],
  members:   ['forge', 'Settings', 'Members'],
  settings:  ['forge', 'Settings'],
};

function App() {
  const [route, setRoute] = useState('dashboard');
  const [theme, setTheme] = useState('dark');

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  let view;
  if (route === 'dashboard') view = <Dashboard />;
  else if (route === 'services') view = <Services />;
  else if (route === 'logs') view = <Logs />;
  else if (route === 'settings' || route === 'members') view = <Settings />;
  else view = (
    <div className="empty" style={{ marginTop: 40 }}>
      <h3>{route} view</h3>
      <p>Not built in this kit — use Dashboard, Services, Logs, or Settings to see the real components.</p>
    </div>
  );

  return (
    <div className="app-shell">
      <TopBar crumbs={ROUTE_TITLES[route] || ['forge']} theme={theme} onToggleTheme={() => setTheme((t) => t === 'dark' ? 'light' : 'dark')} />
      <Sidebar route={route} onNavigate={setRoute} />
      <main className="app-main">{view}</main>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
