/* global React, Input, Button, Toast, Badge */
const { useState } = React;

function Settings() {
  const [section, setSection] = useState('general');
  const sections = [
    ['general', 'General'],
    ['members', 'Members'],
    ['tokens', 'API tokens'],
    ['billing', 'Billing'],
    ['danger', 'Danger zone'],
  ];
  return (
    <div>
      <div className="page-head">
        <div>
          <h1>Settings</h1>
          <div className="sub">Workspace · forge</div>
        </div>
      </div>
      <div className="settings-layout">
        <nav className="settings-nav">
          {sections.map(([id, label]) => (
            <a key={id} className={section === id ? 'is-active' : ''} onClick={() => setSection(id)}>{label}</a>
          ))}
        </nav>
        <div>
          {section === 'general' && (
            <>
              <div className="settings-section">
                <h2>Workspace</h2>
                <div className="sub">Identifies your workspace across the API and CLI.</div>
                <div className="settings-row">
                  <Input label="Name" defaultValue="Forge" />
                  <Input label="Slug" defaultValue="forge" help="Used in URLs and the CLI." />
                </div>
                <div style={{ display: 'flex', gap: 8 }}>
                  <Button variant="primary">Save changes</Button>
                  <Button variant="ghost">Discard</Button>
                </div>
              </div>
              <div className="settings-section">
                <h2>Defaults</h2>
                <div className="sub">Defaults for new services in this workspace.</div>
                <div className="settings-row">
                  <Input label="Default region" defaultValue="us-east-1" />
                  <Input label="Default branch" defaultValue="main" />
                </div>
              </div>
            </>
          )}
          {section === 'members' && (
            <div className="settings-section">
              <h2>Members</h2>
              <div className="sub">3 people have access to this workspace.</div>
              <table className="ftable">
                <thead><tr><th>name</th><th>email</th><th>role</th><th></th></tr></thead>
                <tbody>
                  <tr><td>Wil Taylor</td><td className="col-mono">wil@forge.io</td><td><Badge tone="accent">Owner</Badge></td><td></td></tr>
                  <tr><td>Lyn Park</td><td className="col-mono">lyn@forge.io</td><td><Badge tone="neutral">Admin</Badge></td><td></td></tr>
                  <tr><td>Rin Okada</td><td className="col-mono">rin@forge.io</td><td><Badge tone="neutral">Member</Badge></td><td></td></tr>
                </tbody>
              </table>
              <div style={{ marginTop: 16 }}><Button variant="primary" icon="plus">Invite member</Button></div>
            </div>
          )}
          {section === 'tokens' && (
            <div className="settings-section">
              <h2>API tokens</h2>
              <div className="sub">Tokens authenticate CLI and CI/CD workflows.</div>
              <div className="empty" style={{ marginTop: 12 }}>
                <h3>No tokens yet</h3>
                <p>Create a token to deploy from CI.</p>
                <div style={{ marginTop: 12 }}><Button variant="primary" icon="plus">Create token</Button></div>
              </div>
            </div>
          )}
          {section === 'billing' && (
            <div className="settings-section">
              <h2>Billing</h2>
              <div className="sub">Plan, usage, invoices.</div>
              <Toast tone="info" icon="info">You're on the Team plan · renews May 12</Toast>
            </div>
          )}
          {section === 'danger' && (
            <div className="settings-section" style={{ borderColor: 'color-mix(in oklab, var(--danger) 30%, var(--border))' }}>
              <h2 style={{ color: 'var(--danger-fg)' }}>Delete workspace</h2>
              <div className="sub">Permanently deletes this workspace and all its services. This can't be undone.</div>
              <Button variant="danger">Delete workspace</Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

window.Settings = Settings;
