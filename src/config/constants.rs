use crate::droptube_dir;

/// exit codes
#[repr(i32)]
pub enum AppExitCode {
    ///No Problems.
    Success = 0,

    /// A catch-all code for general, unclassified runtime errors.
    Error = 1,

    /// Incorrect command line argument usage, invalid flags, or syntax errors.
    UsageError = 2,
}
impl From<AppExitCode> for i32 {
    fn from(code: AppExitCode) -> Self {
        code as i32
    }
}
/// Number of video cards displayed per page on the home feed.
pub const PAGE_SIZE: usize = 24;

/// `\r` (Carriage Return): Moves the blinking cursor back to the absolute beginning (column 0) of
/// the current line. `\x1b[2K` (ANSI Erase Line): Wipes out all the text on the current line,
/// regardless of where the cursor is positioned.
pub const CLEAR_LINE: &str = "\r\x1b[2K";

/// The name of the generated DropTube data directory.
pub const DROPTUBE_DIRECTORY: &str = droptube_dir!("");
