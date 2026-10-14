# 🎬 DropTube

> A lightweight, high-performance command-line utility that transforms any local directory into a touch-optimized,
> mobile streaming interface over your local network. Zero databases. Zero configuration.

[![Language](https://shields.io/badge/Language-Rust-orange.svg)](https://rust-lang.org)
[![Framework](https://shields.io/badge/Framework-Axum-blue.svg)](https://github.com/tokio-rs/axum)
[![Runtime](https://shields.io/badge/Runtime-Tokio-purple.svg)](https://tokio.rs)

### Installation

#### Windows (WinGet)

```powershell
winget install Blessing.DropTube
```

#### Linux (Homebrew)

```bash
brew tap Bless-04/tap
brew install droptube
```

#### Building from Source

Ensure you have the Rust toolchain installed, clone the repository, and build the release binary:

```bash
cargo build --release
```

The executable will be located in `target/release/droptube` (or `target/release/droptube.exe` on Windows).

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

## Profiles, ratings, and tags

Choose a profile from the menu in the header. On your first visit, the chooser suggests **guest**;
browsing and playback work before selection, while personal editing stays inactive. Enter an existing
name or a new name, then choose **Switch profile** whenever you want to use another identity.
Names contain 1–32 ASCII letters or digits, are stored lowercase, and cannot be Windows device names.
Spaces, punctuation, underscores, hyphens, and non-ASCII characters are rejected.

Profiles select shared identities; they do not authenticate users. Anyone choosing the same name
shares its ratings and tags. **guest** is one shared profile across browsers, and LAN IP addresses are
not identities. The selection is remembered by a server-set HttpOnly cookie for one year, rather than
localStorage. Its host-only name is `droptube_profile_<library_id>`, with `Path=/`, `SameSite=Lax`,
and `Secure` when served over HTTPS. A different browser or hostname/IP may need a fresh selection.

The library ID is the lowercase 16-digit FNV-1a 64-bit hash of the canonical root's native encoding,
with a platform discriminator (Unix path bytes or Windows UTF-16 little-endian units), fixed offset
`0xcbf29ce484222325`, and multiplier `0x100000001b3`. It is independent of the port and stable across
restarts. Separate library roots have separate selections; tabs and ports serving the same library on
the same host share the selection.

On a watch page, **Your rating** saves one integer rating from 1 to 5 for that profile. Cards and the
watch view separately show the community average and contribution count. For example, `p1=4` and
`p2=5` produce **4.5/5 · 2 ratings**. Changing p1 to 2 gives **3.5/5 · 2 ratings**: rerating replaces
one contribution, and repeating a rating never adds a vote. Unrated profiles do not count. Videos with
no contributions show **Unrated**; unreadable community metadata shows **Rating unavailable**.

Use **Add tag** to create a label or reuse a suggestion. Tags accumulate; remove buttons delete only
the selected label. Labels are trimmed, lowercased, and limited to 64 Unicode code points, with empty
labels and control characters rejected. Tags, suggestions, tag-aware search, and filters belong to
the active profile; community ratings are shared. Switching profiles with JavaScript keeps the current
player mounted. When another tab switches your profile, pending edits are discarded and you are asked
to retry against the refreshed profile. Without JavaScript, profile selection and browsing/filtering
work through normal forms; personal editing requires JavaScript.

### Storage and API usage

Saved data lives at `<served_library_root>/.droptube/profiles/<profile>/metadata.json`, including for
videos in subfolders. Choosing a profile or reading missing metadata creates no files. The first saved
edit creates an indented JSON file. Each video's ID is its exact library-relative path, with forward
slashes and its extension. Profile files and temporary save files cannot be downloaded through media
or explorer routes. Existing thumbnail caches keep their original layout.

Start DropTube with `droptube --port 18081 --depth 255 /path/to/library`, where the library contains
`movies/movie.mp4`. These commands were verified against an indexed video at that path; the save
assigns p1's personal rating and returns its complete metadata:

```bash
curl -X POST http://localhost:18081/api/user/rating \
  -H "Content-Type: application/json" \
  -H "X-Droptube-Profile: p1" \
  --data '{"video_id":"movies/movie.mp4","rating":4}'

curl http://localhost:18081/api/user/metadata -H "X-Droptube-Profile: p1"
curl http://localhost:18081/api/videos/ratings
```

The save creates `.droptube/profiles/p1/metadata.json` under that library root. Unknown indexed IDs
return 404; invalid names such as `X-Droptube-Profile: ../../etc` return 400 without creating profile
files. Explicit-header clients may omit `expected_profile`; browser edits use the cookie and must
include the profile whose metadata was displayed, receiving 409 if the selection has changed.
The public aggregate endpoint exposes averages/counts only, without profile names or personal tags.

Each profile's updates are serialized and committed using a synced same-directory temporary file
followed by replacement, preserving prior committed data on failed writes. Corrupt data is reported
and preserved rather than overwritten. This guarantees complete committed JSON, not unconditional
power-loss durability. Multiple server processes concurrently writing the same library are unsupported.
Use a trusted local library: containment checks reject existing links, but cannot protect against a
hostile local process racing filesystem replacements. Moving or renaming a video can leave orphaned
metadata; automatic remapping and importing old filename metadata are unsupported.

Filename prefixes such as `[5] [action,drama]` are ordinary title text and never create ratings or tags.
Underscores and dots still become spaces in display titles. Existing same-stem image thumbnails and
FFmpeg-generated thumbnails continue to work. No media files are renamed or rewritten.

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
cargo check
cargo test --all-features --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt -- --check
cargo audit
cargo deny check
cargo +nightly udeps --all-targets
cargo semver-checks --baseline-rev v0.3.0
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
