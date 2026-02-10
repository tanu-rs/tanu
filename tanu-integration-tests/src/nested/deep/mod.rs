pub mod deeper;

use tanu::eyre;

#[tanu::test]
async fn deep_test() -> eyre::Result<()> {
    tanu::check!(true);
    Ok(())
}
