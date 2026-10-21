-- Activity log, event receipts and agent continuations (task 1.4.7).
--
-- An `event_receipts` row is written for every provider delivery, whether or
-- not it matched a trigger. The unique key is the delivery identity, so a
-- provider retry is counted on the existing receipt (`duplicate_count`)
-- instead of producing a second row. `run_public_ids` records what the
-- delivery created, which lets a duplicate return the original runs.
--
-- `agent_conversations` groups arrivals by the thread, issue or pull request
-- they belong to, for one trigger. It stores the agent execution currently
-- serving that conversation so a later arrival can continue it (and steer it
-- when it is busy) or restore it when the agent is gone.
--
-- Runs gained the arrival metadata the activity read API exposes: the provider
-- it came from, the receipt it belongs to, the conversation it continued, how
-- it continued, and the output limits that apply to this arrival alone.

CREATE TABLE IF NOT EXISTS event_receipts (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id         CHAR(36)        NOT NULL,
    organization_id   BIGINT UNSIGNED NOT NULL,
    project_id        BIGINT UNSIGNED NULL,
    project_public_id CHAR(36)        NULL,
    provider          VARCHAR(16)     NOT NULL,
    connection        VARCHAR(64)     NOT NULL,
    delivery_key      VARCHAR(255)    NOT NULL,
    event_type        VARCHAR(128)    NOT NULL,
    action            VARCHAR(64)     NULL,
    resource_kind     VARCHAR(64)     NULL,
    resource_id       VARCHAR(255)    NULL,
    actor             VARCHAR(255)    NULL,
    channel           VARCHAR(255)    NULL,
    status            ENUM('received','matched','unmatched','rejected')
                                      NOT NULL DEFAULT 'received',
    matched_triggers  INT UNSIGNED    NOT NULL DEFAULT 0,
    run_public_ids    LONGTEXT        NULL,
    duplicate_count   INT UNSIGNED    NOT NULL DEFAULT 0,
    received_at       DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at        DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at        DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                      ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_event_receipts_public_id (public_id),
    UNIQUE KEY uk_event_receipts_delivery
        (organization_id, provider, connection, delivery_key),
    KEY idx_event_receipts_project (project_id, received_at),
    KEY idx_event_receipts_status (status, received_at),

    CONSTRAINT fk_event_receipts_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_event_receipts_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS agent_conversations (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id           CHAR(36)        NOT NULL,
    organization_id     BIGINT UNSIGNED NOT NULL,
    project_id          BIGINT UNSIGNED NOT NULL,
    trigger_id          BIGINT UNSIGNED NOT NULL,
    conversation_key    VARCHAR(512)    NOT NULL,
    agent_execution_id  CHAR(36)        NULL,
    run_public_id       CHAR(36)        NULL,
    busy                TINYINT(1)      NOT NULL DEFAULT 0,
    output_limits_json  LONGTEXT        NULL,
    created_at          DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                        ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_agent_conversations_public_id (public_id),
    UNIQUE KEY uk_agent_conversations_trigger_key (trigger_id, conversation_key),
    KEY idx_agent_conversations_project (project_id),

    CONSTRAINT fk_agent_conversations_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_agent_conversations_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_agent_conversations_trigger
        FOREIGN KEY (trigger_id) REFERENCES triggers (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

ALTER TABLE runs
    ADD COLUMN provider VARCHAR(16) NULL AFTER revision_id,
    ADD COLUMN receipt_public_id CHAR(36) NULL AFTER provider,
    ADD COLUMN conversation_key VARCHAR(512) NULL AFTER receipt_public_id,
    ADD COLUMN continuation ENUM('created','continued','restored')
        NOT NULL DEFAULT 'created' AFTER conversation_key,
    ADD COLUMN steered TINYINT(1) NOT NULL DEFAULT 0 AFTER continuation,
    ADD COLUMN reused_execution_id CHAR(36) NULL AFTER steered,
    ADD COLUMN output_limits_json LONGTEXT NULL AFTER reused_execution_id,
    ADD KEY idx_runs_provider (provider),
    ADD KEY idx_runs_continuation (continuation);
