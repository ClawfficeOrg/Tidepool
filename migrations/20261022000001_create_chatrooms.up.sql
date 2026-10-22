-- Chatrooms: rooms, members and messages (task 1.5.1).
--
-- A room belongs to one project inside one organization. `origin_pool` names
-- the pool a room was federated from; it is NULL for every v1 room, because
-- federation (Rhumb) is deferred to v3. The column exists now so federation
-- does not need a data migration later.
--
-- A member is either a pool user or an agent session, never both. MySQL
-- refuses a CHECK constraint over a column that also carries a foreign-key
-- referential action (error 3823), so that rule is enforced by
-- `entities::chatroom::Principal` and by the two nullable columns, which carry
-- matching unique keys so one principal joins a room once.
--
-- A message's `id` is the server-defined ordering key. MySQL never reuses an
-- AUTO_INCREMENT value, so `id` is stable and monotonic per server; history is
-- read by `(room_id, id)` and `public_id` never participates in ordering.
-- `reply_to_message_id` is a self-reference and is nulled if the parent goes
-- away, so a reply can never point at a deleted row.
--
-- `author_user_id` is nulled when the pool user is deleted, so the message
-- survives with no author. Readers render that as a deleted author rather than
-- losing the history.

CREATE TABLE IF NOT EXISTS chat_rooms (
    id              BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id       CHAR(36)        NOT NULL,
    organization_id BIGINT UNSIGNED NOT NULL,
    project_id      BIGINT UNSIGNED NOT NULL,
    name            VARCHAR(120)    NOT NULL,
    visibility      ENUM('private','unlisted','public') NOT NULL DEFAULT 'private',
    origin_pool     VARCHAR(255)    NULL,
    created_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP
                                    ON UPDATE CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_chat_rooms_public_id (public_id),
    UNIQUE KEY uk_chat_rooms_project_name (project_id, name),
    KEY idx_chat_rooms_project (project_id, created_at),
    KEY idx_chat_rooms_organization (organization_id),

    CONSTRAINT fk_chat_rooms_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_chat_rooms_project
        FOREIGN KEY (project_id) REFERENCES projects (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS chat_room_members (
    id             BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id      CHAR(36)        NOT NULL,
    room_id        BIGINT UNSIGNED NOT NULL,
    member_kind    ENUM('user','session') NOT NULL,
    user_id        BIGINT UNSIGNED NULL,
    session_id     CHAR(36)        NULL,
    role           ENUM('owner','member','viewer') NOT NULL DEFAULT 'member',
    joined_at      DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_chat_room_members_public_id (public_id),
    UNIQUE KEY uk_chat_room_members_room_user (room_id, user_id),
    UNIQUE KEY uk_chat_room_members_room_session (room_id, session_id),
    KEY idx_chat_room_members_user (user_id, joined_at),

    CONSTRAINT fk_chat_room_members_room
        FOREIGN KEY (room_id) REFERENCES chat_rooms (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_chat_room_members_user
        FOREIGN KEY (user_id) REFERENCES pool_users (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE IF NOT EXISTS chat_messages (
    id                  BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    public_id           CHAR(36)        NOT NULL,
    room_id             BIGINT UNSIGNED NOT NULL,
    author_kind         ENUM('user','session') NOT NULL,
    author_user_id      BIGINT UNSIGNED NULL,
    author_session_id   CHAR(36)        NULL,
    body                MEDIUMTEXT      NOT NULL,
    reply_to_message_id BIGINT UNSIGNED NULL,
    created_at          DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_chat_messages_public_id (public_id),
    KEY idx_chat_messages_room_id (room_id, id),
    KEY idx_chat_messages_room_time (room_id, created_at),
    KEY idx_chat_messages_author_user (author_user_id, created_at),
    KEY idx_chat_messages_reply_to (reply_to_message_id),

    CONSTRAINT fk_chat_messages_room
        FOREIGN KEY (room_id) REFERENCES chat_rooms (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE,
    CONSTRAINT fk_chat_messages_author_user
        FOREIGN KEY (author_user_id) REFERENCES pool_users (id)
        ON DELETE SET NULL
        ON UPDATE CASCADE,
    CONSTRAINT fk_chat_messages_reply_to
        FOREIGN KEY (reply_to_message_id) REFERENCES chat_messages (id)
        ON DELETE SET NULL
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
