//! Ordered execution tests
//!
//! Tests for the ordered test execution feature. These tests verify that:
//! 1. Tests marked with #[tanu::test(ordered)] in a module run in source order
//! 2. Tests run serially within the same ordered module
//! 3. Different ordered modules can run in parallel

use std::sync::atomic::{AtomicUsize, Ordering};
use tanu::eyre;
use tokio::time::{sleep, Duration};

// Global counter for tracking execution order
static ORDER_COUNTER: AtomicUsize = AtomicUsize::new(0);
static MODULE_A_COUNTER: AtomicUsize = AtomicUsize::new(0);
static MODULE_B_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Module with ordered tests - should run in source order (top to bottom)
#[tanu::test(ordered)]
mod setup_tests {
    use super::*;

    #[tanu::test]
    async fn step_1_init() -> eyre::Result<()> {
        let val = ORDER_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(50)).await;

        // Should be first test (val == 0)
        tanu::check_eq!(0, val, "step_1_init should run first");
        Ok(())
    }

    #[tanu::test]
    async fn step_2_setup() -> eyre::Result<()> {
        let val = ORDER_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(50)).await;

        // Should run after step_1 (val == 1)
        tanu::check_eq!(1, val, "step_2_setup should run second");
        Ok(())
    }

    #[tanu::test]
    async fn step_3_verify() -> eyre::Result<()> {
        let val = ORDER_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(50)).await;

        // Should run after step_2 (val == 2)
        tanu::check_eq!(2, val, "step_3_verify should run third");
        Ok(())
    }

    #[tanu::test]
    async fn step_4_cleanup() -> eyre::Result<()> {
        let val = ORDER_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(50)).await;

        // Should run last (val == 3)
        tanu::check_eq!(3, val, "step_4_cleanup should run last");
        Ok(())
    }
}

/// Another ordered module - should run in parallel with setup_tests
#[tanu::test(ordered)]
mod module_a_tests {
    use super::*;

    #[tanu::test]
    async fn a_test_1() -> eyre::Result<()> {
        let val = MODULE_A_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(30)).await;
        let new_val = MODULE_A_COUNTER.load(Ordering::SeqCst);

        // Within this module, tests should run serially
        tanu::check_eq!(val + 1, new_val);
        Ok(())
    }

    #[tanu::test]
    async fn a_test_2() -> eyre::Result<()> {
        let val = MODULE_A_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(30)).await;
        let new_val = MODULE_A_COUNTER.load(Ordering::SeqCst);

        tanu::check_eq!(val + 1, new_val);
        Ok(())
    }
}

/// Yet another ordered module - should run in parallel with others
#[tanu::test(ordered)]
mod module_b_tests {
    use super::*;

    #[tanu::test]
    async fn b_test_1() -> eyre::Result<()> {
        let val = MODULE_B_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(30)).await;
        let new_val = MODULE_B_COUNTER.load(Ordering::SeqCst);

        tanu::check_eq!(val + 1, new_val);
        Ok(())
    }

    #[tanu::test]
    async fn b_test_2() -> eyre::Result<()> {
        let val = MODULE_B_COUNTER.fetch_add(1, Ordering::SeqCst);
        sleep(Duration::from_millis(30)).await;
        let new_val = MODULE_B_COUNTER.load(Ordering::SeqCst);

        tanu::check_eq!(val + 1, new_val);
        Ok(())
    }
}
