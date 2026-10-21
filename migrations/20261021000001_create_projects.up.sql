-- Projects and configuration revisions (task 1.4.1).
--
-- A project holds one active configuration revision. A revision is an
-- immutable authored file set plus a version number, an active flag and the
-- validation result. The exact authored files are stored verbatim (path plus
-- raw content), never a normalized form.
--
-- `configuration_revisions` has no `updated_at`: rows are write-once. The only
-- field that ever changes is `active`, toggled when a newer revision activates
-- (the previous active revision is deactivated at the same time).
--
-- Projects are organization-scoped: every query filters on `organization_id`
-- and the slug unique key is `(organization_id, slug)`.

CREATE TABLE IF NOT EXISTS projects (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id       CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    slug            VARCHAR(63)     NOT NULL,
    name            VARCHAR(128)    NOT NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_projects_public_id (public_id),
    UNIQUE KEY uk_projects_organization_slug (organization_id, slug),

    CONSTRAINT fk_projects_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS configuration_revisions (
    id                BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id         CHAR(36)        NOT NULL,
    project_id        BIGINT UNSIGNED NOT NULL,
    version           INT UNSIGNED    NOT NULL,
    status            ENUM('valid','invalid') NOT NULL,
    active            TINYINT(1)      NOT NULL DEFAULT 0,
    validation_errors TEXT            NULL,
    created_at        DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_configuration_revisions_public_id (public_id),
    UNIQUE KEY uk_configuration_revisions_project_version (project_id, version),
    KEY idx_configuration_revisions_project_active (project_id, active),

    CONSTRAINT fk_configuration_revisions_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS configuration_revision_files (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    revision_id BIGINT UNSIGNED NOT NULL,
    file_order  INT UNSIGNED    NOT NULL,
    path        VARCHAR(512)    NOT NULL,
    content     LONGTEXT        NOT NULL,

    PRIMARY KEY (id),
    UNIQUE KEY uk_configuration_revision_files_path (revision_id, path),

    CONSTRAINT fk_configuration_revision_files_revision
        FOREIGN KEY (revision_id) REFERENCES configuration_revisions (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
