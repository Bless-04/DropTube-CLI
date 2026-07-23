fn main() {
    println!("Hello, world!");
enum VideoFormat {
    Mp4,
    Mkv,
    Webm,
    Mov,
    Avi,
    M4v,
}
    // Validate directory
    if !movie_directory.exists() {
        eprintln!("\x1b[1;31mError:\x1b[0m Directory '{}' does not exist.", raw_dir);
        std::process::exit(1);
    }
    if !movie_directory.is_dir() {
        eprintln!("\x1b[1;31mError:\x1b[0m '{}' is not a directory.", raw_dir);
        std::process::exit(1);
    }
}
