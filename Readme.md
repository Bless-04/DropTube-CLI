# 🎬 DropTube

> A stateless, high-performance command-line utility that transforms any local directory into a touch-optimized, mobile streaming interface over your local network. Zero databases. Zero configuration.

[![Language](https://shields.io/badge/Language-Rust-orange.svg)](https://rust-lang.org)
[![Framework](https://shields.io/badge/Framework-Axum-blue.svg)](https://github.com/tokio-rs/axum)
[![Runtime](https://shields.io/badge/Runtime-Tokio-purple.svg)](https://tokio.rs)
### Installation
Ensure you have the Rust toolchain installed, clone the repository, and build the release binary:

```bash
cargo build --release
```
Your optimized standalone executable will be located in `target/release/droptube`.

### Usage
Drop a directory path directly into the command-line argument when executing the program:

```bash
# Windows
droptube.exe C:\Users\YourName\Movies

# macOS / Linux
./droptube /path/to/your/movies
```

If no directory argument is passed, DropTube safely defaults to serving your current active terminal directory (`.`).

---
* **Language:** Rust (2024 Edition)
