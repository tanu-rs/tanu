pub mod compression;
pub mod cookie;
pub mod delay;
pub mod delete;
pub mod encoding;
pub mod get;
pub mod head;
pub mod header;
pub mod masking;
pub mod patch;
pub mod post;
pub mod put;
pub mod redirects;
pub mod status_code;
pub mod streaming;
pub mod utility;

use tanu::eyre;

#[tanu::test]
async fn test_in_mod_rs() -> eyre::Result<()> {
    Ok(())
}
