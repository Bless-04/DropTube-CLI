use log::error;
use std::path::Display;

/// Prints the canonical directory being served.
pub fn serving_dir(canonical_dir: Display<'_>) {
    println!("📂 Serving Directory : \x1b[1;34m{}\x1b[0m", canonical_dir);
}

/// Prints the scanning depth to the console.
///
/// - `0` means top-level only (no subdirectory recursion).
/// - `1–254` means recurse up to that many levels deep.
/// - `255` means unlimited recursion.
pub fn scanning_mode(depth: u8) {
    let mode = match depth {
        0 => "Immediate Directory Only (depth: 0)".to_string(),
        u8::MAX => "Recursive — Unlimited Depth".to_string(),
        n => format!("Recursive — Max Depth: {n}"),
    };
    println!("⚙️  Scanning Mode     : \x1b[1;33m{mode}\x1b[0m");
}

/// Prints the local and LAN access URLs.
pub fn local_urls(local_ip_addr: String, port: u16) {
    println!(
        "🚀 Local Access      : \x1b[1;35mhttp://localhost:{}\x1b[0m",
        port
    );

    let network_url = format!("http://{}:{}", local_ip_addr, port);
    println!("📱 Mobile Stream LAN : \x1b[1;35m{}\x1b[0m\n", network_url);

    // Print QR code for the network URL
    if let Err(e) = qr2term::print_qr(&network_url) {
        error!("Failed to generate QR code: {}", e);
    }
}

/// title for cli
pub fn title() {
    println!("\n\x1b[1;36m============================================================\x1b[0m");
    println!("🎬 \x1b[1;32mDropTube\x1b[0m - Local Media Server");
    println!("\x1b[1;36m============================================================\x1b[0m");
}
