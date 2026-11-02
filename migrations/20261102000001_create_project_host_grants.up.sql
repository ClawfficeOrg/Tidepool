-- Per host and project `jobs.tkrun` grants (phase 1.7, task 1.7.3).
--
-- An operator grants one host the right to run tkrun for one project. The
-- grant is pinned to the checkout's tkrun config hash at grant time; a hash the
-- host later reports differently is meant to pause dispatch (not built yet).
-- Revoking deletes the row.

CREATE TABLE IF NOT EXISTS project_host_grants (
    project_id   BIGINT UNSIGNED NOT NULL,
    host_id      BIGINT UNSIGNED NOT NULL,
    capability   VARCHAR(64)     NOT NULL,
    config_hash  CHAR(64)        NOT NULL,
    granted_at   DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (project_id, host_id, capability),
    CONSTRAINT fk_project_host_grants_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_project_host_grants_host
        FOREIGN KEY (host_id) REFERENCES hosts (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
