# 🍝 Bambana, seto!

*“Bambana, seto!” (“You’re wasting your time on childish things, you know!”) — A tiny time tracker that doesn’t waste your time.*

**Bambana, seto!** is a lightweight desktop time tracking app written in Rust.
It is designed to be simple, offline-first, and free from browser-based or cloud-heavy workflows.

***

## ✨ What it does

- Track work sessions with a task name and optional description
- Start/stop timing with a clean, minimal interface
- Save all data locally in SQLite
- Run as a native Windows app with no browser required

***

## 🚀 Why it exists

This project is for people who want to track time without adding complexity.
It avoids unnecessary modes, hidden syncing, and bloated UI patterns.

***

## ✅ Features

- **Offline-first**: works without internet access
- **Native desktop app**: no browser, no web UI, no background browser processes
- **Minimal interface**: only the controls you need
- **Local storage**: SQLite database in a file you control
- **Configurable**: change locale, log path, database path, and UI scaling
- **MIT licensed**: free to use, modify, and redistribute

***

## 🧩 Supported platforms

- ✅ Windows (current)
- 🔜 Linux (planned)
- 🔜 macOS (planned)

> The current release targets Windows.

***

## 📦 Download & Run

### Download

Get the latest release from the GitHub Releases page.

### Run

No installer is required.
Run the executable directly and start tracking.

***

## 🧭 Quick guide

- Type the task name or description in the text field.
- Press the **▶** button to start tracking.
- While the timer is running, the button changes to **⏹**.
- Press **⏹** to stop the session.
- If you press **Enter** while the timer is running, the current session description is updated.
- Click the elapsed time display to edit the session start time.
- If the app detects a period of inactivity, it shows an idle popup with options to keep, discard, or continue tracking.
- Completed sessions appear in the table below, grouped by date.

***

### Build from source

If you want to build the app yourself:

```bash
cargo build --release
```

Then run the generated executable from `target/release`.

***

## ⚙️ Configuration

On Windows, the app stores its configuration using `confy` under:

```
%APPDATA%\bambana-seto\default-config.toml
```

The default configuration is:

```toml
log_path = ".data/bambana.log"
database_path = ".data/bambana.db"
locale = "en"
pixels_per_point = 1.2
idle_period_secs = 600
```

- `log_path`: path for the log file
- `database_path`: path for the SQLite database file
- `locale`: supported values are `en` and `it`
- `pixels_per_point`: UI scaling value for high-DPI screens
- `idle_period_secs`: Configurable idle timeout

A relative path like `.data/bambana.db` is resolved from the application’s working directory.

***

## 🗂️ Data storage

The app saves tracking data in a local SQLite database.
The default database file is:

```
./.data/bambana.db
```

You can open this file with:

- DB Browser for SQLite
- SQLite CLI
- Any SQLite-compatible library or script

***

## 🤝 Contributing

Contributions are welcome.
Please feel free to:

- open issues
- suggest improvements
- submit pull requests

Keep changes aligned with the project’s minimalist, fast, and local-first philosophy.

***

## 📜 License

This project is licensed under the **MIT License**.
See the `LICENSE` file for details.

***

## ❤️ Philosophy

> Software should help you focus — not steal your attention.

**Bambana, seto!** exists to track time without becoming another thing that wastes it.
