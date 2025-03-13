pub mod get;

use tanu::eyre;

#[tanu::test]
async fn test_in_mod_rs() -> eyre::Result<()> {
    Ok(())
}
