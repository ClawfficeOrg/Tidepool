-- Installed triggers and their revisions (task 1.4.3).
--
-- A trigger is one document (event, filters, run) identified per project by a
-- unique name. Installing the same name again replaces the document without
-- changing the trigger's `public_id`, so every revision stays one readable
-- history.
--
-- Revisions are write-once: a successful install appends one row and flips
-- `active` off on the previous row for the same trigger. A document that fails
-- validation writes nothing, so a later failure never undoes an earlier
-- success. `document` holds the canonical JSON of the installed document.
--
-- Trimming and case folding happen before the write (utf8mb4_unicode_ci is
-- case-insensitive), so 'Auto-fix' and 'auto-fix' are the same trigger.

CREATE TABLE IF NOT EXISTS triggers (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id   CHAR(36)        NOT NULL,
    project_id  BIGINT UNSIGNED NOT NULL,
    name        VARCHAR(255)    NOT NULL,
    created_at  DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_triggers_public_id (public_id),
    UNIQUE KEY uk_triggers_project_name (project_id, name),

    CONSTRAINT fk_triggers_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS trigger_revisions (
    id          BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id   CHAR(36)        NOT NULL,
    trigger_id  BIGINT UNSIGNED NOT NULL,
    version     INT UNSIGNED    NOT NULL,
    active      TINYINT(1)      NOT NULL DEFAULT 0,
    document    LONGTEXT        NOT NULL,
    created_at  DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_trigger_revisions_public_id (public_id),
    UNIQUE KEY uk_trigger_revisions_trigger_version (trigger_id, version),
    KEY idx_trigger_revisions_trigger_active (trigger_id, active),

    CONSTRAINT fk_trigger_revisions_trigger
        FOREIGN KEY (trigger_id) REFERENCES triggers (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
