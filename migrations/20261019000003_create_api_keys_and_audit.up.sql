-- Organization-scoped API keys and the hub audit trail (task 1.1.3).
--
-- The full key is `<prefix>.<secret>` and is returned once at creation. Only
-- `prefix` (lookup half) and an Argon2id hash of `secret` are stored.
--
-- `scopes` is a JSON array of the allow-listed scope strings. Revocation sets
-- `status = 'revoked'` and `revoked_at`; authentication only ever selects
-- active rows, so revocation is effective on the next request.
--
-- Every hub mutation writes one `audit_events` row whose actor is a member, an
-- API key or the system (first-run setup/bootstrap).

CREATE TABLE IF NOT EXISTS api_keys (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id       CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    name            VARCHAR(128)    NOT NULL,
    prefix          VARCHAR(16)     NOT NULL,
    secret_hash     VARCHAR(255)    NOT NULL,
    scopes          TEXT            NOT NULL,
    status          ENUM('active','revoked') NOT NULL DEFAULT 'active',
    created_by      BIGINT UNSIGNED NULL,
    last_used_at    DATETIME        NULL,
    revoked_at      DATETIME        NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_api_keys_public_id (public_id),
    UNIQUE KEY uk_api_keys_prefix (prefix),
    KEY idx_api_keys_organization_id (organization_id),
    KEY idx_api_keys_status (status),

    CONSTRAINT fk_api_keys_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_api_keys_created_by
        FOREIGN KEY (created_by) REFERENCES pool_users (id)
        ON DELETE SET NULL
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS audit_events (
    id               BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    organization_id  BIGINT UNSIGNED NOT NULL,
    actor_kind       ENUM('member','api_key','system') NOT NULL,
    actor_public_id  VARCHAR(64)     NULL,
    actor_label      VARCHAR(255)    NULL,
    action           VARCHAR(64)     NOT NULL,
    target_kind      VARCHAR(64)     NULL,
    target_public_id VARCHAR(64)     NULL,
    detail           JSON            NULL,
    created_at       DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    KEY idx_audit_events_organization_id (organization_id),
    KEY idx_audit_events_actor (actor_kind, actor_public_id),
    KEY idx_audit_events_created_at (created_at),

    CONSTRAINT fk_audit_events_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
