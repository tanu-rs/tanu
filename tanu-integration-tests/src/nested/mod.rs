pub mod deep;

use tanu::eyre;

#[tanu::test]
async fn nested_test() -> eyre::Result<()> {
    tanu::check!(true);
    Ok(())
}
