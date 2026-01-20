// Second test runner demonstrating that tanu works from different entry points

// Import shared modules from the main crate
#[path = "../../assertion.rs"]
mod assertion;

mod subset_tests;

use tanu::eyre;

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    println!("Running tests from second binary entry point");

    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await?;
    Ok(())
}
