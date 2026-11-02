-- Action queue bookkeeping (agent 1).

-- 1 while the worker is sending the action. A newer action on the same field
-- may drop an older one only when it is not in flight. Reset on startup.
ALTER TABLE action_queue ADD COLUMN in_flight INTEGER NOT NULL DEFAULT 0;
-- Human text for the footer's failed list, e.g. "Comment on o/r#12".
ALTER TABLE action_queue ADD COLUMN summary TEXT NOT NULL DEFAULT '';
