//! Tests for the `test_ignore` project setting.
//!
//! `tanu.toml` lists `ignored_test` and the `2` case of `ignored_case` in the
//! `docker` project's `test_ignore`. They fail when they run under that
//! project, so a broken filter fails the suite. In the TUI they are listed
//! dimmed as `ignored`.

use tanu::{check, eyre};

const DOCKER_PROJECT: &str = "docker";

#[tanu::test]
async fn ignored_test() -> eyre::Result<()> {
    let project = tanu::get_config();
    check!(
        project.name != DOCKER_PROJECT,
        "test listed in test_ignore must not run in project {}",
        project.name
    );
    Ok(())
}

/// Only the `2` case is ignored; the `1` case still runs.
#[tanu::test(1)]
#[tanu::test(2)]
async fn ignored_case(n: u32) -> eyre::Result<()> {
    let project = tanu::get_config();
    check!(
        n != 2 || project.name != DOCKER_PROJECT,
        "case listed in test_ignore must not run in project {}",
        project.name
    );
    Ok(())
}
