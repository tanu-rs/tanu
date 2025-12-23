# Repository Guidelines

Tanu is an async-friendly WebAPI testing framework for Rust with CLI and TUI modes.

## Project Structure & Module Organization
```
.
├── tanu/                    # Main crate (CLI + API entry point)
├── tanu-core/               # Core runner, HTTP client, assertions, config, reporters
├── tanu-derive/             # Proc macros (#[tanu::test], #[tanu::main])
├── tanu-tui/                # Terminal UI (Elm-style Model-Update-View)
├── tanu-integration-tests/  # Integration test suite (requires Docker)
├── examples/                # Sample tests/configs (see examples/tanu.toml)
├── docs/                    # MkDocs content and assets
├── vendors/                 # Vendored third-party assets
├── Cargo.toml               # Workspace definition
└── mkdocs.yml               # Docs site config
```

### Key Modules
- `tanu-core/src/runner.rs` - Test orchestration engine with event-driven architecture
- `tanu-core/src/http.rs` - HTTP client wrapper around reqwest with automatic logging
- `tanu-core/src/assertion.rs` - Custom assertion macros (check!, check_eq!, etc.)
- `tanu-core/src/config.rs` - TOML configuration loading with env var support
- `tanu-core/src/reporter.rs` - Pluggable test result formatters (List, Table, Null)
- `tanu-derive/src/lib.rs` - Procedural macros for test discovery

## Build, Test, and Development Commands
- `cargo build --workspace` builds all crates in the workspace.
- `cargo test --workspace` runs Rust unit tests across crates.
- `cargo fmt --all` formats Rust code using rustfmt defaults.
- `cargo clippy --workspace --all-targets --all-features` runs lint checks.
- `cargo build --workspace --all-targets --all-features` builds all targets with full feature coverage.
- `cargo run -p tanu-integration-tests -- test` runs the integration suite with the tanu runner (requires Docker for the httpbin container).
- `cargo run -p tanu-integration-tests -- tui` launches the interactive TUI test runner.
- `cargo run -p tanu-integration-tests -- ls` lists all available test cases.
- `mkdocs serve` previews documentation locally (uses Python deps from `pyproject.toml`).

## CLI Commands & Options
- **test**: Run tests in CLI mode with results in terminal.
  - `-p, --projects` - Filter by project names (comma-separated)
  - `-m, --modules` - Filter by module names (comma-separated)
  - `-t, --tests` - Filter by test names (comma-separated)
  - `-c, --concurrency` - Max parallel tests
  - `--capture-http` - Log HTTP requests/responses
- **tui**: Interactive TUI mode for test execution.
  - `-c, --concurrency` - Max parallel tests (default: CPU cores)
- **ls**: List all available test cases.

## Coding Style & Naming Conventions
- Use Rust 2021 style with rustfmt defaults (4-space indentation, no tabs).
- Use `snake_case` for modules/functions, `CamelCase` for types/traits, and `SCREAMING_SNAKE_CASE` for constants.
- Test functions should be `async`, return a `Result`, and use `#[tanu::test]` or parameterized `#[tanu::test(...)]` attributes.

## Testing Guidelines
- Prefer `tanu::check!`, `check_eq!`, and related macros for assertions.
- Group related tests by module (for example, `tanu-integration-tests/src/http/`).
- Use descriptive test names; parameterized tests auto-generate names based on arguments unless a custom name is provided.

### Assertion Macros
- `check!(condition)` - Boolean assertion with optional message
- `check_eq!(left, right)` - Equality assertion with pretty-printing
- `check_ne!(left, right)` - Inequality assertion
- `check_str_eq!(left, right)` - String comparison with detailed diff

### Test Patterns
```rust
// Simple test
#[tanu::test]
async fn simple_test() -> eyre::Result<()> { Ok(()) }

// Parameterized test (generates multiple test cases)
#[tanu::test(200)]
#[tanu::test(404)]
async fn test_status(code: u16) -> eyre::Result<()> { Ok(()) }

// Custom name for parameterized test
#[tanu::test(val1, val2; "custom_name")]
async fn with_name(a: i32, b: i32) -> eyre::Result<()> { Ok(()) }
```

### Task-Local Context
- Use `tanu::get_config()` to access current project configuration
- Use `tanu::scope_current()` when spawning Tokio tasks to inherit test context

## Commit & Pull Request Guidelines
- Commit messages are short, imperative, and include an emoji prefix.
- Format: `<emoji> <imperative summary> (#<issue-or-pr>)` (omit the issue/PR suffix if none).
- Common emojis in this repo:

| Emoji | Meaning            |
| ----- | ------------------ |
|   🐶  | New feature        |
|   🐛  | Bug fix            |
|   🧪  | Tests              |
|   ✨  | Refactoring        |
|   📚  | Documentation      |
|   🔖  | Release/versioning |
- PRs should include a brief summary, testing notes, and links to related issues.
- When modifying code, run `cargo clippy --workspace --all-targets --all-features` and `cargo build --workspace --all-targets --all-features`, and note results in the PR.
- Include screenshots or GIFs for user-facing CLI/TUI changes and doc updates that affect visuals.

## Configuration & Docs Tips
- Example runner configuration lives in `tanu-sample.toml` and `tanu-integration-tests/tanu.toml`.
- When adding a new crate, update the workspace members in `Cargo.toml`.

### Configuration File Structure (`tanu.toml`)
```toml
[tui]
payload.color_theme = "apathy"

[[projects]]
name = "default"
base_url = "https://api.example.com"
timeout = 30000
test_ignore = ["slow_test"]

[retry]
count = 3
factor = 2.0
jitter = false
min_delay = "1s"
max_delay = "60s"
```

### Environment Variable Support
- Global: `TANU_<KEY>=value`
- Project-specific: `TANU_<PROJECT>_<KEY>=value`

## Architecture Notes
- **Event-driven**: Test events published via broadcast channels for real-time UI updates
- **Compile-time test discovery**: Uses `inventory` crate + procedural macros
- **Panic recovery**: `AssertUnwindSafe` + `catch_unwind()` for graceful failure handling
- **Pluggable reporters**: Trait-based system (`Reporter` trait) for custom output formats
- **Concurrency**: Semaphore-based control with configurable parallelism
