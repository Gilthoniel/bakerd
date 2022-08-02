
CREATE TABLE prices (
  base TEXT NOT NULL,
  quote TEXT NOT NULL,
  bid DECIMAL NOT NULL DEFAULT 0,
  ask DECIMAL NOT NULL DEFAULT 0,

  PRIMARY KEY (base, quote)
);
