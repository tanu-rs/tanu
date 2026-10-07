pub mod deeper;

use tanu::eyre;

/// A test two module levels deep is discovered and run.
#[tanu::test]
async fn deep_test() -> eyre::Result<()> {
    tanu::check!(true);
    Ok(())
}
