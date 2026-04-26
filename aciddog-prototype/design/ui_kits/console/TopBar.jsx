/* global React, Icon, Kbd */
const { useState, useEffect } = React;

function TopBar({ crumbs = [], theme, onToggleTheme }) {
  const [paletteOpen, setPaletteOpen] = useState(false);
  useEffect(() => {
    const fn = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setPaletteOpen(true);
      }
      if (e.key === 'Escape') setPaletteOpen(false);
    };
    window.addEventListener('keydown', fn);
    return () => window.removeEventListener('keydown', fn);
  }, []);
  return (
    <header className="ftopbar">
      <div className="ftopbar-brand">
        <img src="../../assets/logo-wordmark.svg" alt="forge" />
      </div>
      <span className="sep" style={{ color: 'var(--fg-3)' }}>/</span>
      <div className="ftopbar-crumbs">
        {crumbs.map((c, i) => (
          <React.Fragment key={i}>
            {i > 0 && <span className="sep">/</span>}
            <span style={{ color: i === crumbs.length - 1 ? 'var(--fg-0)' : 'var(--fg-1)' }}>{c}</span>
          </React.Fragment>
        ))}
      </div>
      <div className="ftopbar-search" onClick={() => setPaletteOpen(true)}>
        <Icon name="search" size={12} />
        <span>Search or jump to…</span>
        <Kbd>⌘K</Kbd>
      </div>
      <button className="ftopbar-icon-btn" onClick={onToggleTheme} title="Toggle theme">
        <Icon name={theme === 'light' ? 'moon' : 'sun'} size={14} />
      </button>
      <button className="ftopbar-icon-btn"><Icon name="bell" size={14} /></button>
      <div style={{ width: 24, height: 24, borderRadius: 999, background: 'var(--bg-3)', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, color: 'var(--fg-1)', border: '1px solid var(--border)' }}>WT</div>
      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} />}
    </header>
  );
}

function CommandPalette({ onClose }) {
  const items = [
    { icon: 'box', label: 'Go to Services' },
    { icon: 'terminal', label: 'Go to Logs' },
    { icon: 'play', label: 'Deploy current branch' },
    { icon: 'refresh', label: 'Restart api-gateway' },
    { icon: 'settings', label: 'Open Settings' },
  ];
  return (
    <div onClick={onClose} style={{
      position: 'fixed', inset: 0, background: 'rgb(0 0 0 / 0.5)',
      backdropFilter: 'blur(4px)', zIndex: 50,
      display: 'flex', alignItems: 'flex-start', justifyContent: 'center',
      paddingTop: '15vh',
    }}>
      <div onClick={(e) => e.stopPropagation()} style={{
        width: 520, background: 'var(--bg-4)', border: '1px solid var(--border-strong)',
        borderRadius: 8, overflow: 'hidden',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 14px', borderBottom: '1px solid var(--border-subtle)' }}>
          <Icon name="search" size={14} />
          <input autoFocus placeholder="Search or run a command…" style={{
            flex: 1, background: 'transparent', border: 0, outline: 'none',
            color: 'var(--fg-0)', fontSize: 14, font: 'inherit',
          }} />
          <Kbd>esc</Kbd>
        </div>
        <div style={{ padding: 6 }}>
          {items.map((it, i) => (
            <div key={i} style={{
              display: 'flex', alignItems: 'center', gap: 10,
              padding: '8px 10px', borderRadius: 4, fontSize: 13,
              color: 'var(--fg-0)',
              background: i === 0 ? 'var(--bg-2)' : 'transparent',
              cursor: 'pointer',
            }}>
              <Icon name={it.icon} size={14} style={{ opacity: .8 }} />
              <span>{it.label}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

window.TopBar = TopBar;
