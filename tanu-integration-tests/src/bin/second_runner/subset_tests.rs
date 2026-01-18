use tanu::{check, eyre};

#[tanu::test]
async fn test_from_second_binary() -> eyre::Result<()> {
    check!(true);
    println!("Test running from second binary!");
    Ok(())
}
