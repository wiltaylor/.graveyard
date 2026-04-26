/* global React, Icon */
const { useState } = React;

const NAV = [
  { section: 'Workspace' },
  { id: 'dashboard', label: 'Dashboard', icon: 'home' },
  { id: 'services',  label: 'Services',  icon: 'box', count: 12 },
  { id: 'logs',      label: 'Logs',      icon: 'terminal' },
  { id: 'metrics',   label: 'Metrics',   icon: 'activity' },
  { id: 'database',  label: 'Database',  icon: 'database' },
  { section: 'Configuration' },
  { id: 'environments', label: 'Environments', icon: 'git-branch' },
  { id: 'members',      label: 'Members',      icon: 'users' },
  { id: 'settings',     label: 'Settings',     icon: 'settings' },
];

function Sidebar({ route, onNavigate }) {
  return (
    <nav className="fsidebar">
      {NAV.map((it, i) =>
        it.section
          ? <div key={i} className="fsidebar-section">{it.section}</div>
          : (
            <a key={it.id}
               className={route === it.id ? 'is-active' : ''}
               onClick={() => onNavigate(it.id)}>
              <Icon name={it.icon} size={14} />
              <span>{it.label}</span>
              {it.count != null && <span className="count">{it.count}</span>}
            </a>
          )
      )}
    </nav>
  );
}

window.Sidebar = Sidebar;
