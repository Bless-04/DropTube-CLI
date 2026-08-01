/// Default Port for server
pub const DEFAULT_PORT: u16 = 8081;

/// `\r` (Carriage Return): Moves the blinking cursor back to the absolute beginning (column 0) of
/// the current line. `\x1b[2K` (ANSI Erase Line): Wipes out all the text on the current line,
/// regardless of where the cursor is positioned.
pub const CLEAR_LINE: &str = "\r\x1b[2K";

/// The file path of the html page
pub const HTML_SOURCE: &str = include_str!("../../public/index.html");
