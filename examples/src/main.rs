mod parameterized;
mod simple;

use tanu::eyre;

#[tanu::main]
#[tokio::main]
async fn main() -> eyre::Result<()> {
    let runner = run();
    let app = tanu::App::new();
    app.run(runner).await?;
    Ok(())
}
