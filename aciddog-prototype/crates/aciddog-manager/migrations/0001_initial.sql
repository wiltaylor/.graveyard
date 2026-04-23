CREATE TABLE instances (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    transport    TEXT NOT NULL CHECK (transport IN ('https','unix')),
    url          TEXT,
    socket_path  TEXT,
    api_key      TEXT,
    tls_ca_path  TEXT,
    created_at   TEXT NOT NULL
);
