//! Opt-in terminal dashboard for a running DropTube server.

mod event;
mod runtime;
mod startup;
mod ui;

pub use runtime::run_with_logs;
pub use ui::TuiConfig;
