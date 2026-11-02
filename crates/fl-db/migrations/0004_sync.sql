-- Action queue bookkeeping (agent 1).

-- 1 while the worker is sending the action. A newer action on the same field
-- may drop an older one only when it is not in flight. Reset on startup.
ALTER TABLE action_queue ADD COLUMN in_flight INTEGER NOT NULL DEFAULT 0;
-- Human text for the footer's failed list, e.g. "Comment on o/r#12".
ALTER TABLE action_queue ADD COLUMN summary TEXT NOT NULL DEFAULT '';

-- Notification actions: `notifications.id` is GitHub's thread id, a numeric
-- string. `action_queue.target_id` and `pending_fields.row_id` store it as
-- CAST(id AS INTEGER) (not the rowid, which VACUUM may change). Pending
-- state for an inbox row:
--   SELECT field FROM pending_fields
--   WHERE table_name = 'notifications' AND row_id = CAST(n.id AS INTEGER)
-- Fields: 'unread', 'done'.
