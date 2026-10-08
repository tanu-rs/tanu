use tanu::eyre;

/// A test three module levels deep is discovered and run.
#[tanu::test]
async fn deepest_test() -> eyre::Result<()> {
    tanu::check!(true);
    Ok(())
}
