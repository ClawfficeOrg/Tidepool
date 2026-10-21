-- Workflow runs and their ordered steps (task 1.4.6).
--
-- A run is created only after every step's environment resolved and every host
-- is connected, so an offline dispatch leaves no row behind.
--
-- `delivery_key` is the deduplication key, unique per trigger: an event retry
-- or a manual resubmission returns the existing run instead of starting a
-- second one.
--
-- `execution_id` is the deterministic create-intent id for one step. It is
-- stored before the intent is sent, so a lost response or a process restart
-- re-sends the same id and the host answers with the existing execution.
-- `workflow_json`, `environments_json` and `partials_json` are the dispatch-time
-- snapshot, so recovery evaluates exactly the configuration the run started
-- from.

CREATE TABLE IF NOT EXISTS runs (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id         CHAR(36)        NOT NULL,
    organization_id   BIGINT UNSIGNED NOT NULL,
    project_id        BIGINT UNSIGNED NOT NULL,
    project_public_id CHAR(36)        NOT NULL,
    trigger_id        BIGINT UNSIGNED NOT NULL,
    trigger_public_id CHAR(36)        NOT NULL,
    trigger_name      VARCHAR(255)    NOT NULL,
    workflow_name     VARCHAR(255)    NOT NULL,
    revision_id       CHAR(36)        NULL,
    origin            ENUM('event','manual') NOT NULL,
    actor             VARCHAR(255)    NULL,
    delivery_key      VARCHAR(255)    NOT NULL,
    request_text      MEDIUMTEXT      NOT NULL,
    inputs_json       LONGTEXT        NOT NULL,
    workflow_json     LONGTEXT        NOT NULL,
    environments_json LONGTEXT        NOT NULL,
    partials_json     LONGTEXT        NOT NULL,
    status            ENUM('pending','running','succeeded','failed','interrupted')
                                      NOT NULL DEFAULT 'pending',
    deadline_at       DATETIME        NULL,
    started_at        DATETIME        NULL,
    completed_at      DATETIME        NULL,
    created_at        DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at        DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                      ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_runs_public_id (public_id),
    UNIQUE KEY uk_runs_trigger_delivery (trigger_id, delivery_key),
    KEY idx_runs_status (status),
    KEY idx_runs_project (project_id),

    CONSTRAINT fk_runs_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS run_steps (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id      CHAR(36)        NOT NULL,
    run_id         BIGINT UNSIGNED NOT NULL,
    step_id        VARCHAR(128)    NOT NULL,
    position       INT UNSIGNED    NOT NULL,
    status         ENUM('pending','running','succeeded','failed','skipped','interrupted')
                                   NOT NULL DEFAULT 'pending',
    condition_met  TINYINT(1)      NULL,
    execution_id   CHAR(36)        NULL,
    environment    VARCHAR(255)    NULL,
    host_public_id CHAR(36)        NULL,
    directory      TEXT            NULL,
    branch         VARCHAR(255)    NULL,
    prompt         MEDIUMTEXT      NULL,
    output         MEDIUMTEXT      NULL,
    error          TEXT            NULL,
    reason         VARCHAR(255)    NULL,
    started_at     DATETIME        NULL,
    completed_at   DATETIME        NULL,
    created_at     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                   ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_run_steps_public_id (public_id),
    UNIQUE KEY uk_run_steps_run_step (run_id, step_id),
    UNIQUE KEY uk_run_steps_execution (execution_id),
    KEY idx_run_steps_run_position (run_id, position),

    CONSTRAINT fk_run_steps_run
        FOREIGN KEY (run_id) REFERENCES runs (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
