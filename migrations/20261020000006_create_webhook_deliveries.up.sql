-- GitHub webhook deliveries (task 1.3.2).
--
-- The unique (connection_public_id, delivery_id) key is the dedup guarantee:
-- GitHub retries carry the same X-GitHub-Delivery id, and only the first insert
-- is recorded. `connection_public_id` rather than a FK also covers
-- environment-managed connections.

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id                   BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    organization_id      BIGINT UNSIGNED NOT NULL,
    connection_public_id VARCHAR(64)     NOT NULL,
    provider             VARCHAR(16)     NOT NULL,
    delivery_id          VARCHAR(128)    NOT NULL,
    event                VARCHAR(64)     NOT NULL,
    action               VARCHAR(64)     NULL,
    result               ENUM('accepted','ignored') NOT NULL DEFAULT 'accepted',
    received_at          DATETIME        NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (id),
    UNIQUE KEY uk_webhook_deliveries_connection (connection_public_id, delivery_id),
    KEY idx_webhook_deliveries_org (organization_id, received_at),

    CONSTRAINT fk_webhook_deliveries_organization
        FOREIGN KEY (organization_id) REFERENCES organizations (id)
        ON DELETE CASCADE
        ON UPDATE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
