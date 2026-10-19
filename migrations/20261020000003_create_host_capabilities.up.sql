-- Host execution grant and capability report (task 1.2.4).
--
-- Enrollment alone never permits the pool to start agents. `execution_granted`
-- is the separate `host.execute` grant, default false. Until it is set the host
-- is connected-only.
--
-- A capability report is a flat list of provider/model/mode/thinking variants.
-- Re-reporting replaces the previous list atomically. Provider-native options
-- are opaque JSON supplied by the host.

ALTER TABLE hosts
    ADD COLUMN execution_granted TINYINT(1) NOT NULL DEFAULT 0 AFTER status;

CREATE TABLE IF NOT EXISTS host_capabilities (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    host_id         BIGINT UNSIGNED NOT NULL,
    provider        VARCHAR(64)     NOT NULL,
    model           VARCHAR(128)    NOT NULL,
    execution_mode  VARCHAR(64)     NOT NULL,
    thinking_option VARCHAR(64)     NULL,
    options         TEXT            NULL,
    reported_at     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    KEY idx_host_capabilities_host_id (host_id),
    KEY idx_host_capabilities_provider (host_id, provider),

    CONSTRAINT fk_host_capabilities_host
        FOREIGN KEY (host_id) REFERENCES hosts (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
