-- Host enrollment and relationship credentials (task 1.2.2).
--
-- Enrollment is a two-step exchange:
--   1. an operator or API key mints a ten-minute, single-use token bound to a
--      registered host;
--   2. the host generates its own long-lived credential and exchanges the
--      token for a stored relationship. The pool stores only the credential's
--      Argon2id hash and never returns it again.
--
-- `host_credentials.host_id` is UNIQUE: one host has at most one relationship.
-- `enrollment_tokens.issued_by_api_key_id` lets API-key revocation expire the
-- tokens that key issued.

CREATE TABLE IF NOT EXISTS host_credentials (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    host_id         BIGINT UNSIGNED NOT NULL,
    credential_hash VARCHAR(255)    NOT NULL,
    last_used_at    DATETIME        NULL,
    revoked_at      DATETIME        NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_host_credentials_host_id (host_id),

    CONSTRAINT fk_host_credentials_host
        FOREIGN KEY (host_id) REFERENCES hosts (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS enrollment_tokens (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id           CHAR(36)        NOT NULL,
    organization_id     BIGINT UNSIGNED NOT NULL,
    host_id             BIGINT UNSIGNED NOT NULL,
    token_hash          VARCHAR(255)    NOT NULL,
    issued_by_api_key_id BIGINT UNSIGNED NULL,
    expires_at          DATETIME        NOT NULL,
    consumed_at         DATETIME        NULL,
    revoked_at          DATETIME        NULL,
    created_at          DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_enrollment_tokens_public_id (public_id),
    KEY idx_enrollment_tokens_host_id (host_id),
    KEY idx_enrollment_tokens_api_key (issued_by_api_key_id),
    KEY idx_enrollment_tokens_expires_at (expires_at),

    CONSTRAINT fk_enrollment_tokens_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_enrollment_tokens_host
        FOREIGN KEY (host_id) REFERENCES hosts (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_enrollment_tokens_api_key
        FOREIGN KEY (issued_by_api_key_id) REFERENCES api_keys (id)
        ON DELETE SET NULL
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
