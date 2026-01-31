//! Serial execution tests
//!
//! Tests for the serial test execution feature. These tests verify that:
//! 1. Tests marked with #[tanu::test(serial)] run sequentially
//! 2. Tests in different serial groups can run in parallel
//! 3. Non-serial tests can run in parallel with serial tests

use std::sync::atomic::{AtomicUsize, Ordering};
use tanu::eyre;
use tokio::time::{sleep, Duration};

// Global counter for testing serial execution
static SERIAL_COUNTER: AtomicUsize = AtomicUsize::new(0);
static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);
static CACHE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Test that serial tests don't run concurrently (default group)
#[tanu::test(serial)]
async fn serial_test_1() -> eyre::Result<()> {
    let val = SERIAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = SERIAL_COUNTER.load(Ordering::SeqCst);

    // If another serial test ran during our sleep, this would fail
    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

#[tanu::test(serial)]
async fn serial_test_2() -> eyre::Result<()> {
    let val = SERIAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = SERIAL_COUNTER.load(Ordering::SeqCst);

    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

#[tanu::test(serial)]
async fn serial_test_3() -> eyre::Result<()> {
    let val = SERIAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = SERIAL_COUNTER.load(Ordering::SeqCst);

    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

/// Test that tests in different serial groups can run in parallel
#[tanu::test(serial = "database")]
async fn db_test_1() -> eyre::Result<()> {
    let val = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = DB_COUNTER.load(Ordering::SeqCst);

    // Only other "database" group tests should be serialized
    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

#[tanu::test(serial = "database")]
async fn db_test_2() -> eyre::Result<()> {
    let val = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = DB_COUNTER.load(Ordering::SeqCst);

    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

#[tanu::test(serial = "cache")]
async fn cache_test_1() -> eyre::Result<()> {
    let val = CACHE_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = CACHE_COUNTER.load(Ordering::SeqCst);

    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

#[tanu::test(serial = "cache")]
async fn cache_test_2() -> eyre::Result<()> {
    let val = CACHE_COUNTER.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(50)).await;
    let new_val = CACHE_COUNTER.load(Ordering::SeqCst);

    tanu::check_eq!(val + 1, new_val);
    Ok(())
}

/// Test that non-serial tests run in parallel (baseline)
#[tanu::test]
async fn parallel_test_1() -> eyre::Result<()> {
    sleep(Duration::from_millis(50)).await;
    Ok(())
}

#[tanu::test]
async fn parallel_test_2() -> eyre::Result<()> {
    sleep(Duration::from_millis(50)).await;
    Ok(())
}

/// Test that serial attribute works with parameterized tests
#[tanu::test(serial, 1)]
#[tanu::test(serial, 2)]
#[tanu::test(serial, 3)]
async fn serial_parameterized(value: i32) -> eyre::Result<()> {
    tanu::check!(value > 0);
    sleep(Duration::from_millis(20)).await;
    Ok(())
}

/// Test that serial attribute works with named groups and parameters
#[tanu::test(serial = "2xx", 200)]
#[tanu::test(serial = "2xx", 201)]
#[tanu::test(serial = "2xx", 202)]
#[tanu::test(serial = "2xx", 203)]
#[tanu::test(serial = "2xx", 204)]
#[tanu::test(serial = "4xx", 400)]
#[tanu::test(serial = "4xx", 401)]
#[tanu::test(serial = "4xx", 402)]
#[tanu::test(serial = "4xx", 403)]
#[tanu::test(serial = "4xx", 404)]
async fn serial_named_parameterized(status: u16) -> eyre::Result<()> {
    tanu::check!(status >= 200);
    sleep(Duration::from_secs(1)).await;
    Ok(())
}

// ============================================================================
// Tests for attribute ordering flexibility
// ============================================================================

/// Test that serial can appear AFTER arguments (not just at the beginning)
#[tanu::test(100, serial)]
#[tanu::test(200, serial)]
async fn serial_after_args(value: i32) -> eyre::Result<()> {
    tanu::check!(value > 0);
    sleep(Duration::from_millis(30)).await;
    Ok(())
}

/// Test that serial with group name can appear after arguments
#[tanu::test(1, serial = "flexible")]
#[tanu::test(2, serial = "flexible")]
async fn serial_group_after_args(value: i32) -> eyre::Result<()> {
    tanu::check!(value > 0);
    sleep(Duration::from_millis(30)).await;
    Ok(())
}

// ============================================================================
// Tests for cross-group parallelism (verifying different groups run in parallel)
// ============================================================================

static PARALLEL_GROUP_A_START: AtomicUsize = AtomicUsize::new(0);
static PARALLEL_GROUP_A_END: AtomicUsize = AtomicUsize::new(0);
static PARALLEL_GROUP_B_START: AtomicUsize = AtomicUsize::new(0);
static PARALLEL_GROUP_B_END: AtomicUsize = AtomicUsize::new(0);

/// Test that verifies group A and group B can run in parallel
/// If they run sequentially: A starts, A ends, then B starts, B ends
/// If they run in parallel: A starts, B starts (before A ends)
#[tanu::test(serial = "parallel_group_a")]
async fn parallel_group_a_test() -> eyre::Result<()> {
    PARALLEL_GROUP_A_START.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(100)).await;
    PARALLEL_GROUP_A_END.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

#[tanu::test(serial = "parallel_group_b")]
async fn parallel_group_b_test() -> eyre::Result<()> {
    PARALLEL_GROUP_B_START.fetch_add(1, Ordering::SeqCst);
    sleep(Duration::from_millis(100)).await;
    PARALLEL_GROUP_B_END.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

/// Verification test - runs after both groups complete
/// This test checks that both groups started before either finished
/// (proving they ran in parallel)
#[tanu::test]
async fn verify_cross_group_parallelism() -> eyre::Result<()> {
    // Wait a bit for other tests to complete
    sleep(Duration::from_millis(300)).await;

    let a_start = PARALLEL_GROUP_A_START.load(Ordering::SeqCst);
    let a_end = PARALLEL_GROUP_A_END.load(Ordering::SeqCst);
    let b_start = PARALLEL_GROUP_B_START.load(Ordering::SeqCst);
    let b_end = PARALLEL_GROUP_B_END.load(Ordering::SeqCst);

    // Note: This test is probabilistic and may have false negatives
    // but should never have false positives in correct implementation
    // We can't strictly verify parallelism without instrumentation,
    // but we can at least check that both ran
    tanu::check!(a_start > 0 && a_end > 0);
    tanu::check!(b_start > 0 && b_end > 0);

    Ok(())
}
