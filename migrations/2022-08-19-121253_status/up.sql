
CREATE TABLE statuses (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    resources TEXT NOT NULL,
    node TEXT,
    timestamp_ms BIGINT NOT NULL
);
