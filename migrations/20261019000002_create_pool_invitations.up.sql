-- Team invitations (task 1.1.2).
--
-- The raw invitation token is shown once and never stored: `token_hash` is an
-- Argon2id PHC string for the secret half. The token itself is
-- `<public_id>.<secret>` so acceptance can look the row up first.
--
-- Single use is `accepted_at`; revocation is `revoked_at`; expiry is
-- `expires_at`. All three are checked inside the acceptance transaction.

CREATE TABLE IF NOT EXISTS pool_invitations (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id       CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    email           VARCHAR(255)    NOT NULL,
    role            ENUM('owner','admin','member','viewer')
                                    NOT NULL DEFAULT 'member',
    token_hash      VARCHAR(255)    NOT NULL,
    invited_by      BIGINT UNSIGNED NULL,
    expires_at      DATETIME        NOT NULL,
    accepted_at     DATETIME        NULL,
    revoked_at      DATETIME        NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_pool_invitations_public_id (public_id),
    KEY idx_pool_invitations_organization_id (organization_id),
    KEY idx_pool_invitations_email (email),
    KEY idx_pool_invitations_expires_at (expires_at),

    CONSTRAINT fk_pool_invitations_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_pool_invitations_invited_by
        FOREIGN KEY (invited_by) REFERENCES pool_users (id)
        ON DELETE SET NULL
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
