use tanu::eyre;

#[tanu::test]
async fn deepest_test() -> eyre::Result<()> {
    tanu::check!(true);
    Ok(())
}
