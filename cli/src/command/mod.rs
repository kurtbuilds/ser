pub mod edit;
pub mod generate;
pub mod list;
pub mod logs;
pub mod new;
pub mod restart;
pub mod run;
pub mod show;
pub mod start;
pub mod stop;
pub mod timer;

use anyhow::Result;
use std::io::{self, Write};

/// Perform an action that starts a service, then confirm the service actually
/// stayed up, finishing the caller's in-progress `Doing something...` line with
/// ` done.` or ` failed.`.
///
/// The service managers report only whether they accepted the job, so without
/// the second step a service that dies on startup exits zero.
pub fn run_and_verify(
    action: impl FnOnce() -> Result<()>,
    verify: impl FnOnce() -> Result<()>,
) -> Result<()> {
    // Flush first: verification takes a moment, and on failure the progress
    // message should already be on screen ahead of the error.
    let _ = io::stdout().flush();

    match action().and_then(|()| verify()) {
        Ok(()) => {
            println!(" done.");
            Ok(())
        }
        Err(e) => {
            println!(" failed.");
            Err(e)
        }
    }
}

pub use edit::Edit;
pub use generate::Generate;
pub use list::List;
pub use logs::Logs;
pub use new::New;
pub use restart::Restart;
pub use run::Run;
pub use show::Show;
pub use start::Start;
pub use stop::Stop;
pub use timer::Timer;
