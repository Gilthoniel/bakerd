
ALTER TABLE accounts DROP COLUMN pending_update;
ALTER TABLE accounts DROP COLUMN balance;
ALTER TABLE accounts DROP COLUMN stake;
ALTER TABLE accounts ADD COLUMN available_amount TEXT NOT NULL DEFAULT '0';
ALTER TABLE accounts ADD COLUMN staked_amount TEXT NOT NULL DEFAULT '0';

ALTER TABLE account_rewards DROP COLUMN amount;
ALTER TABLE account_rewards ADD COLUMN amount TEXT NOT NULL DEFAULT '';

DROP INDEX blocks_unique_height_idx;
DROP INDEX blocks_unique_hash_idx;
