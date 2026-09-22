# 🎬 DropTube

> A lightweight, high-performance command-line utility that transforms any local directory into a touch-optimized,
> mobile streaming interface over your local network. Zero databases. Zero configuration.

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
Usage: droptube.exe [OPTIONS] [PATH]

Arguments:
  [PATH]
          Root path for files (default: current directory)

          [default: ./]

Options:
  -r, --max-depth <MAX_DEPTH>
          Maximum subfolder depth to recurse into.

          `0` = current directory only, `255` = unlimited (default: `0`).

          [default: 0]
          [aliases: --depth, --recurse, --recursive]

  -p, --port <PORT>
          Explicit port to listen on. If not provided, defaults to 8081 and scans upward

      --open-tui
          Open the interactive terminal interface while the server runs

          [aliases: --tui, --use-tui]

      --thumbnails
          Generate missing thumbnails with FFmpeg. Will fail at startup if FFmpeg is unavailable

          [aliases: --show-thumbnails, --use-thumbnails, --generate-thumbnails]

      --ffmpeg-path <FFMPEG_PATH>
          FFmpeg executable to use (otherwise resolved from PATH). Requires --thumbnails

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

#### Example Usage

```bash
# Serve movies recursively from subfolders on port 9000
droptube --depth 255 --port 9000 /path/to/movies

# Opt into the terminal dashboard
droptube --open-tui /path/to/movies
```

The terminal dashboard is strictly opt-in. Without `--open-tui`, DropTube keeps its existing
headless command-line behavior. The dashboard lists every client IP observed since startup: green
means the IP currently has a live TCP connection, while red means its last connection has closed.
Use the arrow keys or Tab to move between Dashboard, Logs, and QR Code, or press `1`, `2`, or `3`
to open a page directly. Page Up/Page Down scroll longer client and log lists. Home/End jump through
the client list, while End returns the Logs page to its live tail. Application logs are captured and
rendered by `tui-logger` in the Logs page so they do not overwrite the client list. Press `q`, Escape,
or Ctrl+C to stop the dashboard and server gracefully.

### Optional FFmpeg thumbnails

Thumbnail generation is **off by default**. To enable it, install [FFmpeg](https://ffmpeg.org/download.html) and run:

```bash
droptube --thumbnails --depth 255 /path/to/movies

# Windows: use an explicit executable if FFmpeg is not on PATH
droptube.exe --thumbnails --ffmpeg-path "C:\Tools\ffmpeg\bin\ffmpeg.exe" "D:\Movies"
```

With generation enabled, DropTube checks FFmpeg before scanning and fails at startup if the executable is missing,
unusable, or does not respond within five seconds. Normal operation without the flag never launches FFmpeg or writes
thumbnail files.

Existing same-stem JPG/JPEG/PNG/WebP sidecars take priority. For videos without sidecars, FFmpeg selects a frame from
the first 30 frames and creates a 480×270 JPEG. Short videos are supported. Generated files live in
`.droptube/thumbnails` beside each video, named with the full filename (for example,
`.droptube/thumbnails/movie.mp4.jpg`). The cache is reused while it is newer than the source and regenerated when the
source changes. Keep the flag enabled on subsequent launches to use this generated cache. The `.droptube` data
directory is hidden by default: its dot-prefixed name hides it on macOS/Linux, and DropTube sets the Hidden attribute on
Windows. Existing visible `.droptube` directories are also marked Hidden when their thumbnails are next reused or
generated.

Generation runs one video at a time without blocking the async runtime. Each decode has a 30-second timeout. Unreadable
or corrupt videos log a warning and keep their placeholder; they do not stop the library from loading. The initial scan
and thumbnail generation finish before the server starts, so the first launch can take longer for large libraries.
Background scans run every 30 seconds. Manual refresh waits until scanning and thumbnail generation finish before
reloading the page.

### Search and loading

Type a query and press **Enter** or click the search button. Search is case-insensitive and matches every word across
titles, filenames, folder paths, and tags across the full index, including videos on other pages. Tag filters combine
with search; pagination and watch links retain the filters. Clearing filters returns to the full library.

The feed shows 24 videos per page. Thumbnail requests begin near the viewport, with native lazy loading as a fallback.
The feed does not create video elements or fetch video data; only opening a watch page loads the selected video. Images
and the player reserve their aspect ratios to avoid layout shifts.

The dark interface and its JavaScript/CSS are served from the application, with no CDN or web-font requirement. The
release executable works from any working directory without a `public` folder beside it.

---

## 🏷️ Naming Conventions & Metadata

DropTube extracts optional metadata directly from the filename string.

- **Rating:** Prefix the filename with `[1]` to `[5]` to give it a star rating.
- **Tags:** Prefix the filename with a comma-separated bracket array like `[action,drama]` to tag it.
- **Display Name:** Underscores (`_`) and dots (`.`) are automatically replaced with spaces to format the display name
  beautifully.
- **Thumbnails:** If an image file (e.g., `.jpg`, `.png`, `.webp`) shares the exact same base name as the video file in
  the same directory (e.g., `video.mp4` and `video.jpg`), DropTube will automatically pair them and serve the image as
  the video thumbnail.

**Example:**
`[5] [sci-fi,action] interstellar_movie.mkv`

- **Rating:** ★★★★★
- **Tags:** Sci-Fi, Action
- **Display Name:** "interstellar movie"

---

---

## ⚙️ Tech Stack & Dependencies

- **Language:** Rust (2024 Edition)
- **CLI Parser:** [Clap v4](https://github.com/clap-rs/clap) - CLI argument parser.
- **Web Framework:** [Axum v0.8](https://github.com/tokio-rs/axum) - Ergonomic, routing-centric web framework backed by
  the Tokio team.
- **Asynchronous Runtime:** [Tokio v1](https://github.com/tokio-rs/tokio) - for non-blocking I/O.
- **Filesystem Routing:** [Tower-HTTP v0.7](https://github.com/tower-rs/tower-http) - static asset serving utilities
  handling partial block mapping.
- **Frontend:** Askama templates, embedded CSS, and plain JavaScript. Responsive dark UI, accessible search forms, and
  lazy thumbnails work without external CDNs.

### Development checks

```bash
cargo build
cargo build --release
cargo test --all-features --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
node --check public/index.js
node --test
```

The real-media regression test is explicitly opt-in so the normal test suite does not require FFmpeg. Run it with FFmpeg
on PATH, or set `DROPTUBE_TEST_FFMPEG` to its executable:

```bash
cargo test --test thumbnails -- --ignored
```

This checks actual JPEG generation, short clips, nested/special-character filenames, cached and stale thumbnails,
sidecar preservation, and corrupt-video handling.
