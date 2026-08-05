/// Default Port for server
pub const DEFAULT_PORT: u16 = 8081;

/// Number of video cards displayed per page on the home feed.
pub const PAGE_SIZE: usize = 24;

/// `\r` (Carriage Return): Moves the blinking cursor back to the absolute beginning (column 0) of
/// the current line. `\x1b[2K` (ANSI Erase Line): Wipes out all the text on the current line,
/// regardless of where the cursor is positioned.
pub const CLEAR_LINE: &str = "\r\x1b[2K";

