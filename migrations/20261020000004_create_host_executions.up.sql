-- Host protocol executions (task 1.2.5).
--
-- `execution_id` is supplied by the pool and is the idempotency key: a resent
-- create intent returns the existing row and never starts a second execution.
-- Output is appended as the host observes it; completion records the final
-- status.

CREATE TABLE IF NOT EXISTS host_executions (
    id            BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id     CHAR(36)        NOT NULL,
    host_id       BIGINT UNSIGNED NOT NULL,
    prompt        TEXT            NOT NULL,
    status        ENUM('queued','running','succeeded','failed','interrupted')
                                  NOT NULL DEFAULT 'queued',
    output        MEDIUMTEXT      NULL,
    created_at    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                  ON UPDATE CURRENT_TIMESTAMP,
    completed_at  DATETIME        NULL,

    PRIMARY KEY (id),
    UNIQUE KEY uk_host_executions_public_id (public_id),
    KEY idx_host_executions_host_id (host_id),
    KEY idx_host_executions_status (status),

    CONSTRAINT fk_host_executions_host
        FOREIGN KEY (host_id) REFERENCES hosts (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
