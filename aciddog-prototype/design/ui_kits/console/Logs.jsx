/* global React, Button, Icon, Badge */
const { useState, useEffect, useRef } = React;

const SAMPLE = [
  ['info',  'GET /v1/projects 200 — 12ms'],
  ['info',  'POST /v1/deploys 201 — 184ms'],
  ['debug', 'cache hit: workers/queue:default'],
  ['warn',  'slow query: SELECT * FROM events WHERE ... — 842ms'],
  ['info',  'worker:default picked up job 4f8a (deploy)'],
  ['info',  'build complete · forge-api · 4.2s · 12 files'],
  ['error', 'worker:slow timed out after 30s — retrying'],
  ['info',  'GET /v1/services 200 — 8ms'],
  ['debug', 'flushed 124 metrics to backend'],
  ['info',  'deploy d-3a91 succeeded (api-gateway)'],
];

function Logs() {
  const [lines, setLines] = useState(() => seed());
  const [paused, setPaused] = useState(false);
  const ref = useRef(null);

  function seed() {
    const out = [];
    let t = Date.now() - 60_000;
    for (let i = 0; i < 30; i++) {
      const [lvl, msg] = SAMPLE[Math.floor(Math.random() * SAMPLE.length)];
      t += Math.floor(Math.random() * 4000);
      out.push({ t, lvl, msg });
    }
    return out;
  }

  useEffect(() => {
    if (paused) return;
    const id = setInterval(() => {
      setLines((l) => {
        const [lvl, msg] = SAMPLE[Math.floor(Math.random() * SAMPLE.length)];
        return [...l.slice(-200), { t: Date.now(), lvl, msg }];
      });
    }, 1100);
    return () => clearInterval(id);
  }, [paused]);

  useEffect(() => {
    if (!paused && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines, paused]);

  return (
    <div>
      <div className="page-head">
        <div>
          <h1>Logs</h1>
          <div className="sub">Streaming · all services · last 24h · {lines.length} lines</div>
        </div>
        <div className="page-actions">
          <Button icon={paused ? 'play' : 'pause'} onClick={() => setPaused(!paused)}>
            {paused ? 'Resume' : 'Pause'}
          </Button>
          <Button icon="download">Export</Button>
        </div>
      </div>

      <div style={{ display: 'flex', gap: 8, marginBottom: 12, alignItems: 'center' }}>
        <Badge tone="accent">all services</Badge>
        <Badge tone="neutral">level: ≥debug</Badge>
        <Badge tone="neutral">last 24h</Badge>
        <span style={{ marginLeft: 'auto', fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--fg-2)' }}>
          {paused ? 'paused' : '● live'}
        </span>
      </div>

      <div className="flogs" ref={ref}>
        {lines.map((l, i) => (
          <div key={i} className="flog-line">
            <span className="flog-time">{new Date(l.t).toISOString().slice(11, 19)}</span>
            <span className={`flog-level ${l.lvl}`}>{l.lvl.toUpperCase()}</span>
            <span className="flog-msg">{l.msg}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

window.Logs = Logs;
