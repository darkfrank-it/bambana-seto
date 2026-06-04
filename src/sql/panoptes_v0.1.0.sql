PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS app_version (
    curr_version TEXT PRIMARY KEY,
);
INSERT INTO app_version (curr_version) VALUES ('0.1.0');
-- Contiene i dati catturati sul processo e la finestra attiva.
CREATE TABLE IF NOT EXISTS active_windows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp DATETIME NOT NULL,
    process_name TEXT NOT NULL,
    window_title TEXT NOT NULL
);
-- Rappresenta una sessione di tracciamento con titolo e intervallo temporale.
CREATE TABLE IF NOT EXISTS tracking_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    start_time DATETIME NOT NULL,
    end_time DATETIME,
    duration INTEGER DEFAULT 0 -- durata in secondi
);
-- Eventi che indicano quando è stato avviato o fermato il tracciamento.
CREATE TABLE IF NOT EXISTS session_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    event_type TEXT CHECK(event_type IN ('start', 'stop')) NOT NULL,
    timestamp DATETIME NOT NULL,
    FOREIGN KEY (session_id) REFERENCES tracking_sessions(id)
);
-- Associa finestre attive alle sessioni di tracciamento.
CREATE TABLE  IF NOT EXISTS window_tracking (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL,
    window_id INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES tracking_sessions(id),
    FOREIGN KEY (window_id) REFERENCES active_windows(id)
);