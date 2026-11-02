-- TideTable binding and host jobs (phase 1.7, tasks 1.7.2 and 1.7.3 slice).
--
-- A project binds to one TideTable slug on one hub. The pool never registers
-- slugs: the binding is accepted only when the hub already lists the slug.
--
-- A job is one typed `tkrun` run sent to one connected host over its WebSocket.
-- There is no queue: a job row is written before `start_job` is sent, and
-- `last_seq` lets the pool drop replayed output lines.

CREATE TABLE IF NOT EXISTS project_tidetable_bindings (
    project_id  BIGINT UNSIGNED NOT NULL,
    server      VARCHAR(512)    NOT NULL,
    slug        VARCHAR(64)     NOT NULL,
    bound_at    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (project_id),
    CONSTRAINT fk_project_tidetable_bindings_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS host_jobs (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id     CHAR(36)        NOT NULL,
    host_id       BIGINT UNSIGNED NOT NULL,
    project_id    BIGINT UNSIGNED NOT NULL,
    kind          VARCHAR(32)     NOT NULL DEFAULT 'tkrun',
    checkout      VARCHAR(64)     NOT NULL,
    options       JSON            NOT NULL,
    status        ENUM('queued','running','succeeded','failed',
                       'stopped','killed','refused')
                                  NOT NULL DEFAULT 'queued',
    refusal_code  VARCHAR(64)     NULL,
    refusal_detail VARCHAR(512)   NULL,
    pid           BIGINT          NULL,
    owner         VARCHAR(255)    NULL,
    exit_code     INT             NULL,
    summary       TEXT            NULL,
    output        MEDIUMTEXT      NULL,
    last_seq      BIGINT UNSIGNED NOT NULL DEFAULT 0,
    created_at    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                  ON UPDATE CURRENT_TIMESTAMP,
    completed_at  DATETIME        NULL,

    PRIMARY KEY (id),
    UNIQUE KEY uk_host_jobs_public_id (public_id),
    KEY idx_host_jobs_project_status (project_id, status),
    KEY idx_host_jobs_host_status (host_id, status),

    CONSTRAINT fk_host_jobs_host
        FOREIGN KEY (host_id) REFERENCES hosts (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_host_jobs_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
