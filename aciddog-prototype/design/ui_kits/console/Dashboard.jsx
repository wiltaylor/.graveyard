/* global React, Card, Stat, Badge, Button, Icon, StatusDot, Toast */

function Sparkline({ values, tone = 'accent' }) {
  const w = 120, h = 28, max = Math.max(...values), min = Math.min(...values);
  const norm = (v) => (max === min ? h / 2 : h - ((v - min) / (max - min)) * (h - 2) - 1);
  const pts = values.map((v, i) => `${(i / (values.length - 1)) * w},${norm(v).toFixed(1)}`).join(' ');
  const color = tone === 'danger' ? 'var(--danger)' : tone === 'success' ? 'var(--success)' : 'var(--accent)';
  return (
    <svg width={w} height={h} style={{ display: 'block' }}>
      <polyline fill="none" stroke={color} strokeWidth="1.5" points={pts} />
    </svg>
  );
}

function Dashboard() {
  const reqSeries = [12,14,11,18,22,20,28,24,26,30,28,34];
  const errSeries = [0.04,0.05,0.03,0.04,0.06,0.10,0.09,0.05,0.04,0.03,0.04,0.04];
  const recent = [
    { id: 'd-3a91', service: 'api-gateway', branch: 'main', sha: 'a1b2c3d', author: 'wil', status: 'success', time: '2m ago', dur: '1m 12s' },
    { id: 'd-3a90', service: 'auth',        branch: 'main', sha: '7e8f9a0', author: 'lyn', status: 'success', time: '14m ago', dur: '0m 48s' },
    { id: 'd-3a8f', service: 'workers',     branch: 'fix/timeouts', sha: '4d5e6f7', author: 'rin', status: 'failed',  time: '1h ago', dur: '4m 02s' },
    { id: 'd-3a8e', service: 'api-gateway', branch: 'main', sha: '11223cc', author: 'wil', status: 'success', time: '3h ago', dur: '1m 03s' },
  ];
  return (
    <div>
      <div className="page-head">
        <div>
          <h1>Dashboard</h1>
          <div className="sub">Production · us-east-1 · last sync 12 s ago</div>
        </div>
        <div className="page-actions">
          <Button icon="refresh">Refresh</Button>
          <Button variant="primary" icon="zap">Deploy</Button>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 12, marginBottom: 16 }}>
        <Card padded>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
            <Stat label="Requests / min" value="12,840" delta="+8.2%" tone="success" />
            <Sparkline values={reqSeries} tone="accent" />
          </div>
        </Card>
        <Card padded>
          <Stat label="Latency p95" value="142 ms" delta="−4 ms vs 1h" tone="success" />
        </Card>
        <Card padded>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
            <Stat label="Error rate" value="0.04 %" delta="+0.01" tone="danger" />
            <Sparkline values={errSeries} tone="danger" />
          </div>
        </Card>
        <Card padded>
          <Stat label="Active workers" value="48 / 60" delta="80 % capacity" tone="neutral" />
        </Card>
      </div>

      <div style={{ marginBottom: 16 }}>
        <Toast tone="warning" icon="alert-triangle">
          workers · degraded — p95 above SLO for 6 min. <a href="#">View incident →</a>
        </Toast>
      </div>

      <Card title="Recent deploys" action={<Button variant="ghost" size="sm">View all</Button>}>
        <table className="ftable">
          <thead>
            <tr><th>id</th><th>service</th><th>branch</th><th>author</th><th>status</th><th>duration</th><th>time</th><th></th></tr>
          </thead>
          <tbody>
            {recent.map((d) => (
              <tr key={d.id}>
                <td className="col-mono">{d.id}</td>
                <td>{d.service}</td>
                <td className="col-mono">{d.branch}<span style={{ color: 'var(--fg-3)' }}> · {d.sha}</span></td>
                <td>{d.author}</td>
                <td>
                  {d.status === 'success'
                    ? <Badge tone="success" dot>Success</Badge>
                    : <Badge tone="danger" dot>Failed</Badge>}
                </td>
                <td className="col-mono">{d.dur}</td>
                <td className="col-mono">{d.time}</td>
                <td><Icon name="more-horizontal" size={14} style={{ color: 'var(--fg-2)' }} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </Card>
    </div>
  );
}

window.Dashboard = Dashboard;
