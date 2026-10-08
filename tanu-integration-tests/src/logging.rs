use tanu::eyre;
use tracing::{debug, error, info, trace, warn};

/// Logs at every level, to see how the live reporter and the TUI Logs pane render them.
#[tanu::test]
async fn logs_at_every_level() -> eyre::Result<()> {
    error!("error level log");
    warn!("warn level log");
    info!("info level log");
    debug!("debug level log");
    trace!("trace level log");

    // ANSI colors in a message take precedence over the level style.
    info!("\x1b[32mgreen\x1b[0m, \x1b[1;33mbold yellow\x1b[0m and plain");
    error!("\x1b[36mcyan\x1b[0m in an error");

    // Continuation lines are indented under the message.
    warn!("multi-line log\nsecond line\nthird line");

    Ok(())
}
