-- Reverses 20261021000004_create_activity.
--
-- Conversations and receipts are dropped before their run columns, so the
-- down file stays valid on a server that checks foreign keys strictly.

DROP TABLE IF EXISTS agent_conversations;
DROP TABLE IF EXISTS event_receipts;

ALTER TABLE runs
    DROP KEY idx_runs_continuation,
    DROP KEY idx_runs_provider,
    DROP COLUMN output_limits_json,
    DROP COLUMN reused_execution_id,
    DROP COLUMN steered,
    DROP COLUMN continuation,
    DROP COLUMN conversation_key,
    DROP COLUMN receipt_public_id,
    DROP COLUMN provider;
