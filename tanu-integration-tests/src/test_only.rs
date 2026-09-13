//! Tests for the `test_only` / `test_ignore` project settings.
//!
//! `tanu.toml` defines an `allowlist` project whose `test_only` lists
//! `listed_test_runs` and `listed_but_ignored_test_is_skipped`, and whose
//! `test_ignore` also lists the latter. Tests that must be filtered out fail
//! when they run under that project, so a broken filter fails the suite.

use tanu::{check, eyre};

const ALLOWLIST_PROJECT: &str = "allowlist";

#[tanu::test]
async fn listed_test_runs() -> eyre::Result<()> {
    let project = tanu::get_config();
    check!(
        ["docker", ALLOWLIST_PROJECT].contains(&project.name.as_str()),
        "unexpected project {}",
        project.name
    );
    Ok(())
}

#[tanu::test]
async fn unlisted_test_is_skipped() -> eyre::Result<()> {
    let project = tanu::get_config();
    check!(
        project.name != ALLOWLIST_PROJECT,
        "test not listed in test_only must not run in project {}",
        project.name
    );
    Ok(())
}

#[tanu::test]
async fn listed_but_ignored_test_is_skipped() -> eyre::Result<()> {
    let project = tanu::get_config();
    check!(
        project.name != ALLOWLIST_PROJECT,
        "test listed in both test_only and test_ignore must not run in project {}",
        project.name
    );
    Ok(())
}
