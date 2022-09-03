
ALTER TABLE accounts DROP COLUMN available_amount;
ALTER TABLE accounts DROP COLUMN staked_amount;
ALTER TABLE accounts ADD COLUMN balance TEXT NOT NULL DEFAULT '0';
ALTER TABLE accounts ADD COLUMN stake TEXT NOT NULL DEFAULT '0';
ALTER TABLE accounts ADD COLUMN pending_update BOOLEAN NOT NULL DEFAULT 1 CHECK (pending_update IN (0, 1));

ALTER TABLE account_rewards DROP COLUMN amount;
ALTER TABLE account_rewards ADD COLUMN amount TEXT NOT NULL DEFAULT '0';

-- Add missing unique constraints for blocks.

CREATE UNIQUE INDEX blocks_unique_height_idx ON blocks(height);
CREATE UNIQUE INDEX blocks_unique_hash_idx ON blocks(hash);
