-- Reverses 20261021000003_create_runs.
--
-- Steps cascade with their run, so dropping the child first keeps the down
-- migration valid even on a server that checks foreign keys strictly.

DROP TABLE IF EXISTS run_steps;
DROP TABLE IF EXISTS runs;
