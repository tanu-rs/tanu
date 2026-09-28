# Reporters

Reporters receive events from the test runner — the planned test count, test start, checks, HTTP/gRPC calls, retries, results, and the final summary — and turn them into output. You can enable several at once.

```bash
cargo run -- test --reporters list,allure
```

## Built-in reporters

| Name | Description |
|---|---|
| `live` | Default on an interactive terminal. Keeps a live view at the bottom of the terminal: one line per running test (with elapsed time and retry count) and a progress bar with passed/failed counts. Failures are printed in full above it as they happen, with captured HTTP logs (depending on `--capture-http`), followed by a summary. Passing tests, including ones that passed after a retry, leave nothing behind. With `--capture-http all`, every HTTP/gRPC call appears as one line (method, status, URL, duration, test) in a fixed-height `http` window, and with `--capture-rust`, the latest Rust logs appear in a `logs` window; both disappear when the run ends. Use `--reporters list` to keep full output. |
| `list` | Default when stdout is not a terminal (CI, pipes, files). Prints one line per test as it finishes, followed by captured HTTP logs (depending on `--capture-http`) and a summary. |

When `--reporters` is not given, tanu picks `live` if stdout is a terminal, and `list` if it isn't, if the `CI` environment variable is set, or if `TERM=dumb`. Pass `--reporters list` to get per-test output in a terminal.

## Allure

[tanu-allure](https://github.com/tanu-rs/tanu-allure) writes Allure-compatible JSON for each test. HTTP calls, assertions, and timings appear in the Allure dashboard without extra plumbing.

![Allure report](assets/allure-report.png){ .tanu-shot }

Add the crate:

```toml
[dependencies]
tanu-allure = "0.8"
```

Register the reporter in `main`:

```rust
#[tanu::main]
#[tokio::main]
async fn main() -> tanu::eyre::Result<()> {
    let runner = run();
    let mut app = tanu::App::new();
    app.install_reporter(
        "allure",
        tanu_allure::AllureReporter::with_results_dir("allure-results"),
    );
    app.run(runner).await
}
```

Run the tests with the reporter enabled, then generate or serve the report with the [Allure CLI](https://allurereport.org/docs/install/):

```bash
cargo run -- test --reporters allure,list
allure serve allure-results
```

See the [tanu-allure repository](https://github.com/tanu-rs/tanu-allure) for the latest version and a GitHub Actions workflow that publishes reports to GitHub Pages.

## Writing a custom reporter

Implement the [`Reporter`](https://docs.rs/tanu/latest/tanu/reporter/trait.Reporter.html) trait. Every method has a default implementation, so override only the events you care about:

| Method | Called when |
|---|---|
| `on_start` | A test starts |
| `on_check` | A `check!` macro succeeds or fails |
| `on_call` | An HTTP or gRPC call completes |
| `on_retry` | A failed test is about to be retried |
| `on_end` | A test finishes |
| `on_summary` | All tests have finished |

```rust
use tanu::{async_trait, eyre, reporter::Reporter, runner};

#[derive(Default)]
struct FailureCounter {
    failed: Vec<String>,
}

#[async_trait::async_trait]
impl Reporter for FailureCounter {
    async fn on_end(
        &mut self,
        project: String,
        module: String,
        test_name: String,
        test: runner::Test,
    ) -> eyre::Result<()> {
        if test.result.is_err() {
            self.failed.push(format!("[{project}] {module}::{test_name}"));
        }
        Ok(())
    }

    async fn on_summary(&mut self, summary: runner::TestSummary) -> eyre::Result<()> {
        println!("{} of {} tests failed", summary.failed_tests, summary.total_tests);
        for name in &self.failed {
            println!("  {name}");
        }
        Ok(())
    }
}
```

Install it under a name, and select it with `--reporters`:

```rust
let mut app = tanu::App::new();
app.install_reporter("failures", FailureCounter::default());
app.run(runner).await?;
```

```bash
cargo run -- test --reporters list,failures
```
