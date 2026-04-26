/* global React */
const { useState, useEffect, useRef } = React;

/* ---------------- Icon ----------------------------------------------------- */
function Icon({ name, size = 16, className = '', style = {} }) {
  return (
    <img
      src={`../../assets/icons/${name}.svg`}
      width={size}
      height={size}
      alt=""
      aria-hidden="true"
      className={className}
      style={{ display: 'inline-block', verticalAlign: 'middle', ...style }}
    />
  );
}

/* ---------------- Button --------------------------------------------------- */
function Button({ variant = 'secondary', size = 'md', icon, children, className = '', ...props }) {
  const cls = `fbtn fbtn-${variant} fbtn-${size} ${className}`;
  return (
    <button className={cls} {...props}>
      {icon && <Icon name={icon} size={size === 'sm' ? 12 : 14} />}
      {children}
    </button>
  );
}

/* ---------------- Input ---------------------------------------------------- */
function Input({ icon, error, label, help, ...props }) {
  return (
    <label className="ffield">
      {label && <span className="ffield-label">{label}</span>}
      <span className={`ffield-input ${error ? 'is-error' : ''}`}>
        {icon && <Icon name={icon} size={14} />}
        <input {...props} />
      </span>
      {help && <span className={`ffield-help ${error ? 'is-error' : ''}`}>{help}</span>}
    </label>
  );
}

/* ---------------- Badge ---------------------------------------------------- */
function Badge({ tone = 'neutral', dot, children }) {
  return (
    <span className={`fbadge fbadge-${tone}`}>
      {dot && <span className="fbadge-dot" />}
      {children}
    </span>
  );
}

/* ---------------- Card ----------------------------------------------------- */
function Card({ title, action, children, padded = true, className = '' }) {
  return (
    <section className={`fcard ${className}`}>
      {title && (
        <header className="fcard-head">
          <h3 className="fcard-title">{title}</h3>
          {action}
        </header>
      )}
      <div className={padded ? 'fcard-body' : ''}>{children}</div>
    </section>
  );
}

/* ---------------- Stat ----------------------------------------------------- */
function Stat({ label, value, delta, tone }) {
  return (
    <div className="fstat">
      <div className="eyebrow">{label}</div>
      <div className="fstat-value">{value}</div>
      {delta && <div className={`fstat-delta tone-${tone || 'neutral'}`}>{delta}</div>}
    </div>
  );
}

/* ---------------- Kbd ------------------------------------------------------ */
function Kbd({ children }) { return <kbd className="fkbd">{children}</kbd>; }

/* ---------------- Toast ---------------------------------------------------- */
function Toast({ tone = 'info', icon = 'info', children }) {
  return (
    <div className={`ftoast ftoast-${tone}`}>
      <Icon name={icon} size={14} />
      <span>{children}</span>
    </div>
  );
}

/* ---------------- Status dot ---------------------------------------------- */
function StatusDot({ tone }) {
  return <span className={`fdot fdot-${tone}`} />;
}

/* Expose to other Babel scripts */
Object.assign(window, { Icon, Button, Input, Badge, Card, Stat, Kbd, Toast, StatusDot });
