
CREATE TABLE blocks (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  height BIGINT NOT NULL CHECK (height >= 0),
  hash TEXT NOT NULL,
  slot_time_ms BIGINT NOT NULL CHECK (slot_time_ms >= 0),
  baker BIGINT NOT NULL CHECK (baker >= 0)
);

CREATE INDEX blocks_height_idx
ON blocks (height);

CREATE TABLE accounts (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  address TEXT NOT NULL UNIQUE,
  available_amount TEXT NOT NULL DEFAULT '0',
  staked_amount TEXT NOT NULL DEFAULT '0',
  lottery_power DECIMAL NOT NULL DEFAULT 0
);

CREATE TABLE account_rewards (
  id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES accounts (id),
  block_hash TEXT NOT NULL,
  amount TEXT NOT NULL,
  epoch_ms BIGINT NOT NULL,
  kind TEXT NOT NULL,

  UNIQUE (account_id, block_hash, kind)
);

-- Insert the initial block where the processing will start.
INSERT INTO blocks (height, hash, slot_time_ms, baker)
VALUES (2840311, '994dbdd7f9493286ed05706e154c3366d83281a76bdb7a058a5f4c7859a9f9a8', 1651978740000, 2);
