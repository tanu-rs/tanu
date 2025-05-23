pub mod delete;
pub mod get;
pub mod head;
pub mod patch;
pub mod post;

use tanu::eyre;

#[tanu::test]
async fn test_in_mod_rs() -> eyre::Result<()> {
    Ok(())
}

#[tanu::test]
async fn slow() -> eyre::Result<()> {
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    Ok(())
}
