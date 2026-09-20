/// Runs the terminal dashboard with a bounded application-log history.
///
/// This is the entry point used by `--open-tui`; log records are rendered by Ratatui instead of
/// being written directly over the alternate screen. Pressing `q`, Escape, or Ctrl+C returns
/// control to the caller, which can then gracefully stop the server task.
pub fn run_with_logs(
) -> io::Result<()> {
}
