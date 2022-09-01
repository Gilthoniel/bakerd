
ALTER TABLE accounts ADD COLUMN pending_update BOOLEAN NOT NULL DEFAULT 1 CHECK (pending_update IN (0, 1));
