use std::sync::atomic::{AtomicUsize, Ordering};

use tanu::eyre;

/// Counter to track retry attempts across test invocations.
/// This persists across retry attempts within the same process.
static RETRY_ATTEMPT: AtomicUsize = AtomicUsize::new(0);

/// Test that fails on the first attempt and succeeds on retry.
/// This validates that the retry mechanism correctly re-executes failed tests.
///
/// Note: Retry only works for error returns, not panics.
#[tanu::test]
async fn succeeds_after_retry() -> eyre::Result<()> {
    let attempt = RETRY_ATTEMPT.fetch_add(1, Ordering::SeqCst);

    if attempt == 0 {
        // First attempt - fail with an error to trigger retry
        eyre::bail!("Intentional failure on first attempt");
    }

    // Subsequent attempts succeed
    Ok(())
}
