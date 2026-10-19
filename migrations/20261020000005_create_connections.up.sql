-- Provider connections (task 1.3.1).
--
-- A connection is a provider account the pool may receive events from.
-- Credentials are secret references only: `credential_ref` names an environment
-- variable/secret-store entry, never the secret itself, and is never exposed by
-- a read endpoint.
--
-- `managed_by` is not stored: rows here are database-managed. Environment-
-- configured connections are overlaid by the service and win on slug, labelled
-- `managed_by = environment`.

CREATE TABLE IF NOT EXISTS connections (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id      CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    provider       ENUM('github','slack','discord') NOT NULL,
    slug           VARCHAR(63)     NOT NULL,
    display_name   VARCHAR(128)    NOT NULL,
    credential_ref VARCHAR(255)    NOT NULL,
    status         ENUM('active','disabled') NOT NULL DEFAULT 'active',
    created_at     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at     DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                   ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_connections_public_id (public_id),
    UNIQUE KEY uk_connections_organization_slug (organization_id, slug),
    KEY idx_connections_provider (organization_id, provider),
    KEY idx_connections_status (status),

    CONSTRAINT fk_connections_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS connection_resources (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    connection_id  BIGINT UNSIGNED NOT NULL,
    resource_kind  VARCHAR(32)     NOT NULL,
    external_id    VARCHAR(255)    NOT NULL,
    display_name   VARCHAR(128)    NULL,

    PRIMARY KEY (id),
    UNIQUE KEY uk_connection_resources (connection_id, resource_kind, external_id),
    KEY idx_connection_resources_external (resource_kind, external_id),

    CONSTRAINT fk_connection_resources_connection
        FOREIGN KEY (connection_id) REFERENCES connections (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
