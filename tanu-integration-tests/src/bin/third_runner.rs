// Third test runner demonstrating tanu works from a simple bin file

// Import shared modules from the main crate
#[path = "../assertion.rs"]
mod assertion;

use tanu::{check, eyre};

// Add a simple test directly in this file
#[tanu::test]
async fn test_from_third_binary() -> eyre::Result<()> {
    check!(true);
    println!("Test running from third binary!");
    Ok(())
}

#[tanu::test]
async fn another_test_in_third() -> eyre::Result<()> {
    check!(2 + 2 == 4);
    println!("Math still works in third binary!");
    Ok(())
}

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    println!("Running tests from third binary (simple .rs file)");
    
    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await?;
    Ok(())
}