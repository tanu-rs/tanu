//! # Tanu - High-performance WebAPI Testing Framework
//!
//! Tanu is a high-performance, async-friendly, and ergonomic WebAPI testing framework for Rust.
//! It's designed to be fast, type-safe, and easily extensible with full support for concurrency
//! and async operations.
//!
//! ## Quick Start
//!
//! You can install `tanu` and `tokio` by running the following commands in your terminal:
//! ```bash
//! cargo add tanu
//! cargo add tokio --features full
//! ```
//!
//! Write your first test:
//!
//! ```rust,no_run
//! use tanu::{check, eyre, http::Client};
//!
//! #[tanu::test]
//! async fn get_users() -> eyre::Result<()> {
//!     let client = Client::new();
//!     let response = client
//!         .get("https://api.example.com/users")
//!         .send()
//!         .await?;
//!
//!     check!(response.status().is_success());
//!     Ok(())
//! }
//!
//! #[tanu::main]
//! #[tokio::main]
//! async fn main() -> eyre::Result<()> {
//!     let runner = run();
//!     let app = tanu::App::new();
//!     app.run(runner).await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Key Features
//!
//! - **Async/Await Native**: Full support for async operations without boilerplate
//! - **Type-Safe**: Leverage Rust's type system for robust API testing
//! - **Ergonomic Assertions**: Use `check!`, `check_eq!`, and other assertion macros
//! - **Parameterized Testing**: Test multiple scenarios with different inputs
//! - **Built-in HTTP Client**: No need to set up reqwest or other HTTP clients manually
//! - **Flexible Error Handling**: Supports `eyre::Result`, `anyhow::Result`, and custom error types
//! - **TUI Support**: Interactive terminal interface for test execution
//! - **Concurrent Execution**: Run tests in parallel for better performance
//!
//! ## Error Types
//!
//! Tanu supports various Result types for flexible error handling:
//!
//! - `eyre::Result<()>` (recommended) - Provides colored backtraces and seamless integration
//! - `anyhow::Result<()>` - Compatible with existing anyhow-based code
//! - `std::result::Result<(), E>` - Standard Rust Result type with custom error types
//!
//! ## Examples
//!
//! ### Basic HTTP Test
//!
//! ```rust,no_run
//! use tanu::{check_eq, eyre, http::Client};
//!
//! #[tanu::test]
//! async fn test_api_endpoint() -> eyre::Result<()> {
//!     let client = Client::new();
//!     let response = client
//!         .get("https://httpbin.org/json")
//!         .header("accept", "application/json")
//!         .send()
//!         .await?;
//!
//!     check_eq!(200, response.status().as_u16());
//!
//!     let data: serde_json::Value = response.json().await?;
//!     check!(data.is_object());
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Parameterized Tests
//!
//! ```rust,no_run
//! use tanu::{check_eq, eyre, http::Client};
//!
//! #[tanu::test(200)]
//! #[tanu::test(404)]
//! #[tanu::test(500)]
//! async fn test_status_codes(expected_status: u16) -> eyre::Result<()> {
//!     let client = Client::new();
//!     let response = client
//!         .get(&format!("https://httpbin.org/status/{expected_status}"))
//!         .send()
//!         .await?;
//!
//!     check_eq!(expected_status, response.status().as_u16());
//!     Ok(())
//! }
//! ```

mod app;

// Re-export procedural macros for test and main attributes
pub use tanu_derive::{main, test};

// Re-export error handling crates for user convenience
pub use anyhow;
pub use eyre;
pub use inventory;
pub use pretty_assertions;

// Re-export main application struct
pub use app::App;

// Re-export core functionality
pub use tanu_core::{
    assertion,
    config::{get_config, get_tanu_config, Config, ProjectConfig},
    http,
    reporter::{ListReporter, NullReporter, Reporter, ReporterType, TableReporter},
    runner::{self, scope_current, Runner, TestInfo},
    {check, check_eq, check_ne, check_str_eq},
};

// Type alias for the async test function
pub type AsyncTestFn =
    fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = eyre::Result<()>> + Send + 'static>>;

// Define the test registration structure for inventory
pub struct TestRegistration {
    pub module: &'static str,
    pub name: &'static str,
    pub test_fn: AsyncTestFn,
}

// Collect tests using inventory
inventory::collect!(TestRegistration);
