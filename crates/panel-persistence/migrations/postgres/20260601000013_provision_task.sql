-- Extend the node_operation_tasks.kind CHECK constraint to allow 'provision'.
ALTER TABLE node_operation_tasks DROP CONSTRAINT IF EXISTS node_operation_tasks_kind_check;
ALTER TABLE node_operation_tasks ADD CONSTRAINT node_operation_tasks_kind_check
    CHECK (kind IN ('apply_config','restart','check_health','provision'));
