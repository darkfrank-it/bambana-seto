# AGENTS.md

Guidance for AI coding agents working in this repository.

## What this is

**Bambana, seto!** is a lightweight, offline-first Windows desktop time tracker written in Rust, using `egui`/`eframe` for the UI and SQLite (via `sqlx`) for local storage. Currently Windows-only (depends on `winapi`/`windows` crates for idle detection); Linux/macOS support is planned but not implemented.

## Commands

```bash
cargo check                  # fastest correctness check
cargo build --release        # release build, output in target/release
cargo clippy --all-targets   # lint; this repo is kept at zero clippy warnings
cargo test                   # run the full test suite
cargo test <substring>       # run tests whose name contains <substring>, e.g.:
cargo test idle_sentinel     # all tests in src/capture/idle_sentinel.rs
```

Tests that use a real SQLite pool (`db_manager`, `ui::app`) create a unique temp `.db` file per test and explicitly `pool.close().await` before deleting it — on Windows a file can't be removed while a connection still has it open, so don't remove that `close().await` call when touching those tests.

## Architecture

- **`src/main.rs`** — entry point. Loads `Config` via `confy` (TOML at `%APPDATA%\bambana-seto\default-config.toml`), sets up logging and locale, opens the SQLite pool, starts the idle watcher, and launches the `eframe`/`egui` native window with `MyEguiApp`.
- **`src/config.rs`** — `Config` struct persisted by `confy`. Fields use `#[serde(default = ...)]` so older on-disk configs missing newer fields (e.g. `idle_period_secs`) still deserialize; keep that pattern when adding config fields.
- **`src/database/db_manager.rs`** — owns the `sqlx::SqlitePool` and all SQL. `open_db` creates/migrates the schema on every startup (`CREATE TABLE/INDEX IF NOT EXISTS`, plus an unconditional `DROP INDEX` + recreate for `idx_one_open_session`). That index enforces "only one open session at a time" by indexing a **constant expression** (`ON sessions((1)) WHERE end_time IS NULL`) rather than the nullable `end_time` column directly — SQLite (like standard SQL) never treats two `NULL`s as equal in a unique index, so indexing the column itself would silently allow unlimited concurrent open sessions. If that index creation fails (pre-existing duplicate open sessions from before this fix), it logs a warning instead of failing app startup.
- **`src/capture/idle_sentinel.rs`** — Windows-only idle/suspend detection. `get_last_input()` calls `GetTickCount` (`windows` crate) and `GetLastInputInfo` (`winapi` crate) directly. The actual idle/suspend decision logic lives in `IdleWatcherState` (a plain struct with `on_tick(now, idle_period, get_idle)`), deliberately decoupled from real syscalls and the `tokio` timer so it can be unit-tested with synthetic clocks/inputs — extend the tests there, not by mocking Windows APIs. The suspend-detection heuristic: `GetTickCount`/`GetLastInputInfo` freeze during sleep/hibernate, so a wall-clock (`Utc::now()`) jump much larger than the polling interval is treated as "the system was suspended," and the reported offline duration is anchored to the last real check *before* the jump (not to "now") — don't reintroduce that mistake.
- **`src/ui/app.rs`** — `MyEguiApp`, the single `eframe::App` implementation holding all UI state. Since `egui`'s `update()` is synchronous but DB writes are async, mutating calls (`begin_session`, `end_session`, `apply_new_*_time`, etc.) update in-memory state immediately and fire-and-forget a `tokio::spawn`'d DB write; `session_id_tx`/`session_id_rx` and `idle_return_rx` are the channels used to feed async results (a newly-inserted session id, an idle-period notification) back into the synchronous UI loop.
- **`locales/{en,it}.yml`** + `rust_i18n` (`i18n!("locales", ...)` in `src/lib.rs`) — all user-facing strings go through the `t!("key")` macro; add new strings to both locale files.
- Tests live in `#[cfg(test)] mod tests` at the bottom of the file they cover, not in a separate `tests/` directory.
