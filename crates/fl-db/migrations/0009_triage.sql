-- Local triage state. GitHub has no snooze, so Done and Snooze live only here.
-- An item returns to the inbox when it updates after done_at, or when
-- snoozed_until passes.
ALTER TABLE notifications ADD COLUMN done_at INTEGER;
ALTER TABLE notifications ADD COLUMN snoozed_until INTEGER;
