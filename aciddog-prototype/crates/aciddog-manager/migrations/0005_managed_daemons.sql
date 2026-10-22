-- The CASCADE clause on instances.managed_by_id is DECLARED-ONLY at runtime
-- because PRAGMA foreign_keys is NOT enabled in this codebase (see src/db.rs).
-- delete_with_paired_instance in src/model/managed_daemon.rs is the runtime
-- source of truth for tearing down both rows atomically.

CREATE TABLE managed_daemons (
    id                      TEXT PRIMARY KEY,
    name                    TEXT NOT NULL UNIQUE,
    work_dir                TEXT NOT NULL UNIQUE,
    socket_path             TEXT NOT NULL,
    desired_state           TEXT NOT NULL CHECK (desired_state IN ('stopped', 'running')),
    start_on_manager_boot   INTEGER NOT NULL DEFAULT 0 CHECK (start_on_manager_boot IN (0, 1)),
    idle_timeout_secs       INTEGER NULL,
    start_on_demand         INTEGER NOT NULL DEFAULT 0 CHECK (start_on_demand IN (0, 1)),
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

ALTER TABLE instances
    ADD COLUMN managed_by_id TEXT NULL REFERENCES managed_daemons(id) ON DELETE CASCADE;

CREATE INDEX instances_managed_by_idx ON instances(managed_by_id);
