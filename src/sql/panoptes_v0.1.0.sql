PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS app_version (
    curr_version TEXT PRIMARY KEY,
);
INSERT INTO app_version (curr_version) VALUES ('0.1.0');
