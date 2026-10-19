-- Pool identity: the single organization that owns this pool and its
-- operator accounts.
--
-- v1 is deliberately single-tenant: `organizations.singleton` is a constant
-- with a UNIQUE key, so the database itself refuses a second organization.
-- Multi-tenancy is deferred (see docs/memory.md, 2026-10-19).
--
-- Passwords are Argon2id PHC strings written by the service layer; no
-- plaintext is ever stored. `password_change_required` marks the temporary
-- password created by TIDEBOOT_* so the first sign-in has to replace it.

CREATE TABLE IF NOT EXISTS organizations (
    id                      BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id               CHAR(36)        NOT NULL,
    name                    VARCHAR(128)    NOT NULL,
    -- Always 1; the UNIQUE key is the one-organization-per-pool guarantee.
    singleton               TINYINT         NOT NULL DEFAULT 1,
    bootstrap_source        ENUM('browser','environment')
                                            NOT NULL DEFAULT 'browser',
    bootstrap_completed_at  DATETIME        NULL,
    created_at              DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at              DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                            ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_organizations_public_id (public_id),
    UNIQUE KEY uk_organizations_singleton (singleton)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS pool_users (
    id                       BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id                CHAR(36)        NOT NULL,
    organization_id          BIGINT UNSIGNED NOT NULL,
    email                    VARCHAR(255)    NOT NULL,
    display_name             VARCHAR(128)    NOT NULL,
    password_hash            VARCHAR(255)    NOT NULL,
    role                     ENUM('owner','admin','member','viewer')
                                             NOT NULL DEFAULT 'member',
    password_change_required TINYINT(1)      NOT NULL DEFAULT 0,
    last_login_at            DATETIME        NULL,
    created_at               DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at               DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                             ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_pool_users_public_id (public_id),
    UNIQUE KEY uk_pool_users_email (email),
    KEY idx_pool_users_organization_id (organization_id),
    KEY idx_pool_users_role (role),

    CONSTRAINT fk_pool_users_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
