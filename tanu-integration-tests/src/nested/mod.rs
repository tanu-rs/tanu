pub mod deep;

use tanu::eyre;

/// A test one module level deep is discovered and run.
#[tanu::test]
async fn nested_test() -> eyre::Result<()> {
    tanu::check!(true);
    Ok(())
}
