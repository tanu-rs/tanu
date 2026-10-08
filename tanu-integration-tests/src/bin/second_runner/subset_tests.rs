use tanu::{check, eyre};

/// A test in a second binary's own module is discovered and run.
#[tanu::test]
async fn test_from_second_binary() -> eyre::Result<()> {
    check!(true);
    println!("Test running from second binary!");
    Ok(())
}
