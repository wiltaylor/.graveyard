/* global React, Card, Badge, Button, Icon, StatusDot, Input */
const { useState } = React;

const SERVICES = [
  { name: 'api-gateway',  status: 'success', p95: '142 ms', errors: '0.04 %', deploy: 'v2.4.1', time: '2m ago' },
  { name: 'auth',         status: 'success', p95: '38 ms',  errors: '0.00 %', deploy: 'v1.9.0', time: '14m ago' },
  { name: 'workers',      status: 'warning', p95: '1.20 s', errors: '0.21 %', deploy: 'v0.7.2', time: '1h ago' },
  { name: 'web',          status: 'success', p95: '88 ms',  errors: '0.02 %', deploy: 'v3.1.5', time: '3h ago' },
  { name: 'edge-cache',   status: 'success', p95: '12 ms',  errors: '0.00 %', deploy: 'v0.4.0', time: 'yesterday' },
  { name: 'db-primary',   status: 'success', p95: '4 ms',   errors: '—',      deploy: '—',      time: '—' },
  { name: 'db-replica-1', status: 'success', p95: '6 ms',   errors: '—',      deploy: '—',      time: '—' },
  { name: 'db-replica-2', status: 'danger',  p95: '—',      errors: '—',      deploy: '—',      time: 'yesterday' },
  { name: 'analytics',    status: 'success', p95: '210 ms', errors: '0.05 %', deploy: 'v0.2.1', time: '4h ago' },
  { name: 'webhooks',     status: 'success', p95: '67 ms',  errors: '0.01 %', deploy: 'v1.0.3', time: '2h ago' },
];

function toneFor(s) { return s === 'success' ? 'success' : s === 'warning' ? 'warning' : 'danger'; }
function labelFor(s) { return s === 'success' ? 'Healthy' : s === 'warning' ? 'Degraded' : 'Down'; }

function Services() {
  const [q, setQ] = useState('');
  const filtered = SERVICES.filter((s) => s.name.toLowerCase().includes(q.toLowerCase()));
  return (
    <div>
      <div className="page-head">
        <div>
          <h1>Services</h1>
          <div className="sub">{SERVICES.length} services · 1 degraded · 1 down</div>
        </div>
        <div className="page-actions">
          <Button icon="download">Export</Button>
          <Button variant="primary" icon="plus">New service</Button>
        </div>
      </div>

      <div style={{ display: 'flex', gap: 8, marginBottom: 12, alignItems: 'center' }}>
        <div style={{ flex: 1, maxWidth: 320 }}>
          <Input icon="search" placeholder="Filter services…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <Button size="sm">All envs</Button>
        <Button size="sm">All regions</Button>
      </div>

      <Card padded={false}>
        <table className="ftable">
          <thead>
            <tr><th>service</th><th>status</th><th>p95</th><th>errors</th><th>deploy</th><th>last change</th><th></th></tr>
          </thead>
          <tbody>
            {filtered.map((s) => (
              <tr key={s.name}>
                <td><StatusDot tone={toneFor(s.status)} /> &nbsp;{s.name}</td>
                <td><Badge tone={toneFor(s.status)} dot>{labelFor(s.status)}</Badge></td>
                <td className="col-mono">{s.p95}</td>
                <td className="col-mono">{s.errors}</td>
                <td className="col-mono">{s.deploy}</td>
                <td className="col-mono">{s.time}</td>
                <td><Icon name="chevron-right" size={14} style={{ color: 'var(--fg-2)' }} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  );
}

window.Services = Services;
