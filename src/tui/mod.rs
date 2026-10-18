//! Opt-in terminal dashboard for a running DropTube server.

mod event;
mod profiles;
mod runtime;
mod startup;
mod ui;

pub use profiles::ProfileTuiContext;
pub use runtime::{run_with_logs, run_with_profiles};
pub use ui::TuiConfig;
