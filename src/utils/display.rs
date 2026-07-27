use std::path::Display;

pub fn serving_dir(canonical_dir: Display) {
    println!(
        "📂 Serving Directory : \x1b[1;34m{}\x1b[0m",
        canonical_dir
    );
}


/// Displays the scanning mode
pub fn scanning_mode(recurse: bool) {
    println!(
        "⚙️  Scanning Mode     : \x1b[1;33m{}\x1b[0m",
        if recurse {
            "Recursive"
        } else {
            "Immediate Directory Only"
        }
    );
}

pub fn local_urls(local_ip_addr: String, port: u16) {
    println!(
        "🚀 Local Access      : \x1b[1;35mhttp://localhost:{}\x1b[0m",
        port
    );
    println!(
        "📱 Mobile Stream LAN : \x1b[1;35mhttp://{}:{}\x1b[0m",
        local_ip_addr, port
    );
}