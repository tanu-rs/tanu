//! Tests for the `test_only` / `test_ignore` project settings.
//!
//! `tanu.toml` defines an `allowlist` project whose `test_only` lists
//! `listed_test_runs`, `listed_but_ignored_test_is_skipped` and the
//! `listed_module` module, and whose `test_ignore` also lists
//! `listed_but_ignored_test_is_skipped`. Tests that must be filtered out fail
//! when they run under that project, so a broken filter fails the suite.

use tanu::{check, eyre};

const ALLOWLIST_PROJECT: &str = "allowlist";

/// Listed in `test_only`, so it runs in the `allowlist` project as well as `docker`.
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

/// Not listed in `test_only`, so it must not run in the `allowlist` project.
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

/// Listed in both `test_only` and `test_ignore`, so it must not run in the `allowlist` project.
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

/// Listed by module path, so every test in it runs.
mod listed_module {
    use tanu::{check, eyre};

    use super::ALLOWLIST_PROJECT;

    /// Its module is listed in `test_only`, so it runs in the `allowlist` project.
    #[tanu::test]
    async fn test_in_listed_module_runs() -> eyre::Result<()> {
        let project = tanu::get_config();
        check!(
            ["docker", ALLOWLIST_PROJECT].contains(&project.name.as_str()),
            "unexpected project {}",
            project.name
        );
        Ok(())
    }
}
