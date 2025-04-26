use tanu::eyre;

#[tanu::test]
async fn same_test_name_in_different_modules() -> eyre::Result<()> {
    Ok(())
}
