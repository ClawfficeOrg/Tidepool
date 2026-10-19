-- Pool host registry (task 1.2.1).
--
-- A host is one enrolled machine. `public_id` is the immutable identifier;
-- `slug` is a friendly, mutable handle derived from the hostname and
-- de-duplicated. Renaming a slug never changes the id.
--
-- `revoked_at` ends the relationship from the pool side. Revoked rows are kept
-- for audit and are refused by every lookup.

CREATE TABLE IF NOT EXISTS hosts (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id       CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    slug            VARCHAR(63)     NOT NULL,
    display_name    VARCHAR(128)    NOT NULL,
    hostname        VARCHAR(255)    NOT NULL,
    status          ENUM('pending_approval','connected','offline','revoked')
                                    NOT NULL DEFAULT 'pending_approval',
    revoked_at      DATETIME        NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_hosts_public_id (public_id),
    UNIQUE KEY uk_hosts_organization_slug (organization_id, slug),
    KEY idx_hosts_status (status),

    CONSTRAINT fk_hosts_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
