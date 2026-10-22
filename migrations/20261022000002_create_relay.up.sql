-- Session relay: opt-in publication of agent session events (task 1.5.5).
--
-- Publishing is off unless a `relay_sessions` row says otherwise, so nothing
-- leaves the machine that runs a session until someone asks for it. The row
-- records who opted in and when, and `stopped_at` marks an explicit stop: a
-- stopped session never publishes again.
--
-- `relay_events.sequence` is per session and monotonic. Subscribers backfill
-- with `sequence > cursor`, which makes a reconnect idempotent: the same cursor
-- always yields the same window, and a client that repeats a read sees no
-- duplicates.
--
-- `payload_json` is a small JSON object chosen by the publisher from the
-- documented allowlist of event kinds. It deliberately has no column for file
-- contents: the relay carries progress, not artefacts.

CREATE TABLE IF NOT EXISTS relay_sessions (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    organization_id BIGINT UNSIGNED NOT NULL,
    session_id      CHAR(36)        NOT NULL,
    enabled         TINYINT(1)      NOT NULL DEFAULT 0,
    started_at      DATETIME        NULL,
    stopped_at      DATETIME        NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_relay_sessions_org_session (organization_id, session_id),
    KEY idx_relay_sessions_enabled (organization_id, enabled),

    CONSTRAINT fk_relay_sessions_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS relay_events (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id       CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    session_id      CHAR(36)        NOT NULL,
    sequence        BIGINT UNSIGNED NOT NULL,
    kind            VARCHAR(32)     NOT NULL,
    payload_json    TEXT            NOT NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_relay_events_public_id (public_id),
    UNIQUE KEY uk_relay_events_session_sequence (organization_id, session_id, sequence),
    KEY idx_relay_events_session (organization_id, session_id, sequence),

    CONSTRAINT fk_relay_events_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
