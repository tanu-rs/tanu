//! # Tanu Core
//!
//! Core functionality for the tanu WebAPI testing framework.
//!
//! This crate provides the fundamental building blocks for tanu, including:
//! - Test runners and execution logic
//! - HTTP client functionality
//! - Assertion macros and utilities
//! - Configuration management
//! - Test reporting infrastructure
//!
//! ## Architecture (block diagram)
//!
//! ```text
//! +---------------------+      +---------------------+      +---------------------+
//! | test definitions    | ---> | runner (execution) | --->  | reporter (output)   |
//! | #[tanu::test]       |      | + event channel    |       | List/Null/etc.      |
//! +---------------------+      +---------------------+      +---------------------+
//!            |                         ^    ^                         ^
//!            v                         |    |                         |
//! +---------------------+              |    |              +---------------------+
//! | assertion macros    | ---publish---+    +---publish--- | HTTP client + logs  |
//! | check!, check_eq!   |                              |   | req/res capture     |
//! +---------------------+                              |   +---------------------+
//!            ^                                         |
//!            |                                         v
//! +---------------------+                       +---------------------+
//! | config + filters    | <-------------------- | test selection      |
//! | projects/modules    |                       | (project/module)    |
//! +---------------------+                       +---------------------+
//! ```
//!
//! Most users should use the main `tanu` crate rather than importing `tanu-core` directly.

#[doc(hidden)]
pub mod assertion;
pub mod config;
pub mod error;
pub mod http;
pub mod reporter;
#[doc(hidden)]
pub mod runner;

// Re-export procedural macros
pub use tanu_derive::{main, test};

// Re-export error handling crates
pub use anyhow;
pub use eyre;

/// Type alias for project names in tanu configuration.
///
/// Project names are used to organize tests into different environments
/// or configurations (e.g., "staging", "production", "development").
/// Each project can have its own configuration settings including
/// base URLs, timeouts, and retry policies.
pub type ProjectName = String;

/// Type alias for module names in test organization.
///
/// Module names correspond to Rust module paths and are used to
/// group related tests together. For example, "api", "auth", "users".
/// They're used for filtering and organizing test output.
pub type ModuleName = String;

/// Type alias for individual test names.
///
/// Test names identify specific test functions within a module.
/// Combined with module and project names, they provide unique
/// identification for each test case in the system.
pub type TestName = String;

// Re-export key functionality
pub use config::{get_config, get_tanu_config, Config, ProjectConfig};
pub use error::{Error, Result};
pub use reporter::{ListReporter, NullReporter, Reporter};
pub use runner::{
    Filter, ModuleFilter, ProjectFilter, Runner, TestIgnoreFilter, TestInfo, TestNameFilter,
};
