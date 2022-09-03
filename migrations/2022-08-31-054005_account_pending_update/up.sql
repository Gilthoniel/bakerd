
ALTER TABLE accounts ADD COLUMN pending_update BOOLEAN NOT NULL DEFAULT 1 CHECK (pending_update IN (0, 1));

-- Add missing unique constraints for blocks.

CREATE UNIQUE INDEX blocks_unique_height_idx ON blocks(height);
CREATE UNIQUE INDEX blocks_unique_hash_idx ON blocks(hash);
