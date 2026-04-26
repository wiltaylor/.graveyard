CREATE TABLE skill_marketplaces (
    id              TEXT PRIMARY KEY,
    git_url         TEXT NOT NULL,
    ref_spec        TEXT,
    last_synced_at  TEXT,
    last_synced_sha TEXT,
    raw_manifest    TEXT
);

CREATE TABLE skills (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    source_kind   TEXT NOT NULL CHECK (source_kind IN ('uploaded','imported','marketplace')),
    source_ref    TEXT,
    description   TEXT,
    version       TEXT,
    content_hash  TEXT NOT NULL,
    upstream_hash TEXT,
    updated_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX skills_source_name ON skills(source_kind, source_ref, name);

CREATE TABLE skill_pushes (
    skill_id     TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
    instance_id  TEXT NOT NULL REFERENCES instances(id) ON DELETE CASCADE,
    pushed_at    TEXT NOT NULL,
    pushed_hash  TEXT NOT NULL,
    PRIMARY KEY (skill_id, instance_id)
);

CREATE INDEX skill_pushes_instance_idx ON skill_pushes(instance_id);
