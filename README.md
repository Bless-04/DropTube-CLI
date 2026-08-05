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

### Command Line Options

DropTube comes with configuration flags to customize its behavior:

```bash
Usage: droptube [OPTIONS] [PATH]

Arguments:
  [PATH]
          Root path for files (default: current directory) [default: ./]

Options:
  -d, --depth <MAX_DEPTH>
          Maximum subfolder depth to recurse into.
          `0` = current directory only, `255` = unlimited (default: `0`). [default: 0]

  -p, --port <PORT>
          Explicit port to listen on. If not provided, defaults to 8081 and scans upward automatically if the port is in use.

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

#### Example Usage
```bash
# Serve movies recursively from subfolders on port 9000
droptube --depth 255 --port 9000 /path/to/movies
```

---

## 🏷️ Naming Conventions & Metadata

DropTube extracts optional metadata directly from the filename string.

* **Rating:** Prefix the filename with `[1]` to `[5]` to give it a star rating.
* **Tags:** Prefix the filename with a comma-separated bracket array like `[action,drama]` to tag it.
* **Display Name:** Underscores (`_`) and dots (`.`) are automatically replaced with spaces to format the display name beautifully.
* **Thumbnails:** If an image file (e.g., `.jpg`, `.png`, `.webp`) shares the exact same base name as the video file in the same directory (e.g., `video.mp4` and `video.jpg`), DropTube will automatically pair them and serve the image as the video thumbnail.

**Example:**
`[5] [sci-fi,action] interstellar_movie.mkv`
* **Rating:** ★★★★★
* **Tags:** Sci-Fi, Action
* **Display Name:** "interstellar movie"

---

---

## ⚙️ Tech Stack & Dependencies

* **Language:** Rust (2024 Edition)
* **CLI Parser:** [Clap v4](https://github.com/clap-rs/clap) - Robust and feature-rich argument parser.
* **Web Framework:** [Axum v0.8](https://github.com/tokio-rs/axum) - Ergonomic, routing-centric web framework backed by the Tokio team.
* **Asynchronous Runtime:** [Tokio v1](https://github.com/tokio-rs/tokio) - Industry standard event-driven architecture for non-blocking I/O.
* **Filesystem Routing:** [Tower-HTTP v0.7](https://github.com/tower-rs/tower-http) - Highly optimized static asset serving utilities handling partial block mapping.
* **Frontend:** Tailwind CSS (via CDN compilation) - Minimal, responsive styles injected dynamically into single-page templates.
