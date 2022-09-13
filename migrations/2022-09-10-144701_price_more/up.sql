DROP TABLE prices;

CREATE TABLE pairs (
  id    INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  base  TEXT NOT NULL,
  quote TEXT NOT NULL,

  UNIQUE (base, quote)
);

CREATE TABLE prices (
  pair_id               INTEGER NOT NULL PRIMARY KEY REFERENCES pairs (id),
  bid                   DECIMAL NOT NULL DEFAULT 0,
  ask                   DECIMAL NOT NULL DEFAULT 0,
  daily_change_relative DECIMAL NOT NULL DEFAULT 0,
  high                  DECIMAL NOT NULL DEFAULT 0,
  low                   DECIMAL NOT NULL DEFAULT 0
);

CREATE TABLE hist_prices (
  pair_id       INTEGER NOT NULL PRIMARY KEY REFERENCES pairs (id),
  bid           DECIMAL NOT NULL DEFAULT 0,
  ask           DECIMAL NOT NULL DEFAULT 0,
  timestamp_ms  BIGINT NOT NULL CHECK (timestamp_ms >= 0)
);
