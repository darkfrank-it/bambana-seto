# 🍝 Bambana, seto!

*"Perdi tempo, sai!" ("You're wasting your time, you know!") — A tiny time tracker that doesn’t waste your time.*

**Bambana, seto!** is a lightweight, no-nonsense desktop time tracking app written in Rust.  

Developed with the help of **GitHub Copilot** and **artificial intelligence** to accelerate deployment while maintaining clarity and simplicity.

It’s built for people who want to track what they do — without dealing with bloated tools, accounts, or browsers.

***

## ✨ Features

### ✅ Works Offline

No internet required. Use it anywhere — even completely disconnected.

### 🖥️ No Browser Required

This is a native desktop application.  
No tabs, no web UI, no background browser processes.

### 🎯 Minimalist Interface

Just the essentials:

* Start / Stop tracking
* Task name
* Description

No distractions. No unnecessary buttons.

### ⚡ Native Performance

Built in Rust with **egui/eframe**.  
Consumes minimal RAM and CPU.

### 💾 Data in Your Hands

All data is stored locally in a **SQLite database**.  
You can:

* Open and inspect it with tools like DB Browser for SQLite
* Modify it with third‑party applications
* Export it to CSV if needed

### 🔗 Easy Integration

Want charts or reports?

* Open the database directly
* Export to CSV and use Excel, Google Sheets, or Python scripts

Your data stays **simple and accessible**.

### 🆓 Free & Open Source

Released under the **MIT License**.  
You are free to use, modify, and redistribute it.

### 🚧 Future: Cross-Platform

* ✅ Windows (current)
* 🔜 Linux support coming
* 🔜 macOS support coming

***

## 🚀 Getting Started

### Download

Grab the latest release from the ../../releases page.

### Run

No installation required:  
Just run the executable and start tracking.

***

## 🗂️ Data Storage

The application stores data in a local SQLite database file.

Typical location:

```
./data/bambana.db
```

You can open it with tools like:

* DB Browser for SQLite
* SQLite CLI
* Any compatible library or script

***

## 🤝 Contributing

Contributions are welcome!  
Feel free to:

* Open issues
* Suggest improvements
* Submit pull requests

Keep it simple, fast, and minimal — that’s the spirit of *Bambana, seto!*.

***

## 📜 License

This project is licensed under the **MIT License**.  
See the `LICENSE` file for details.

***

## ❤️ Philosophy

> Software should help you focus — not steal your attention.

**Bambana, seto!** exists to track time…  
without becoming another thing that wastes it.
