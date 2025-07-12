//! tanu assertion macros.
//!
//! Those assertions are borrowed from `pretty_assetions` crate and made
//! with small modification which throws `Result<_, Error>` instead of
//! panic. The reason for providing own assertion macro is throwing an
//! error allows tanu to be able to print colorized backtrace powered
//! by `eyre`.

/// Custom error type used by assertion macros. This `Error` type is
/// designed to be propagated from test functions using the assertion macros.
/// tanu wraps the error with `eyre::Report` for enhanced error reporting,
/// including the ability to generate and display colorized backtraces.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    StrEq(String),
    #[error("{0}")]
    Eq(String),
    #[error("{0}")]
    Ne(String),
}

/// Asserts that a boolean expression is true.
///
/// This macro provides a non-panicking alternative to `assert!` that integrates
/// with tanu's test reporting system. If the assertion fails, it publishes a
/// failure event and returns an error instead of panicking.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu::{check, http::Client};
///
/// #[tanu::test]
/// async fn test_status_check() -> eyre::Result<()> {
///     let client = Client::new();
///     let response = client.get("https://api.example.com").send().await?;
///     
///     // Basic assertion
///     check!(response.status().is_success());
///     
///     // With custom message
///     check!(response.status() == 200, "Expected 200 OK status");
///     
///     Ok(())
/// }
/// ```
///
/// # Behavior
///
/// - On success: Publishes a success event to the test runner
/// - On failure: Publishes a failure event and returns an `eyre::Error`
/// - Integrates with tanu's reporting system for colored output and detailed logs
#[macro_export]
macro_rules! check {
    ($cond:expr) => {
        $crate::check!(@ $cond, "", "");
    };
    ($cond:expr, $($arg:tt)+) => {
        $crate::check!(@ $cond, ":", $($arg)+);
    };
    (@ $cond:expr, $maybe_colon:expr, $($arg:tt)*) => {
        if !$cond {
            let __message = format!("check failed: {}{}{}", stringify!($cond), $maybe_colon, format_args!($($arg)*));
            let __check = tanu::runner::Check::error(&__message);
            tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
            tanu::eyre::bail!(__message);
        } else {
            let __message = format!("check succeeded: {}{}{}", stringify!($cond), $maybe_colon, format_args!($($arg)*));
            let __check = tanu::runner::Check::success(&__message);
            tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
        }
    };
}

/// Asserts that two string expressions are equal with enhanced string diff output.
///
/// This macro is specifically designed for string comparison and provides
/// superior diff visualization compared to `check_eq!` when dealing with
/// strings, showing character-by-character differences, line differences,
/// and highlighting whitespace issues.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu::{check_str_eq, http::Client};
///
/// #[tanu::test]
/// async fn test_response_text() -> eyre::Result<()> {
///     let client = Client::new();
///     let response = client.get("https://httpbin.org/robots.txt").send().await?;
///     
///     let body = response.text().await?;
///     
///     // Assert string content with detailed diff on failure
///     check_str_eq!("User-agent: *\nDisallow:", body.trim());
///     
///     // With custom message
///     check_str_eq!(expected_json, actual_json, "API response format changed");
///     
///     Ok(())
/// }
/// ```
///
/// # Advantages over `check_eq!` for strings:
///
/// - **Line-by-line diff**: Shows exactly which lines differ
/// - **Character highlighting**: Highlights specific character differences
/// - **Whitespace visualization**: Makes invisible characters visible
/// - **Better formatting**: Optimized display for multi-line strings
///
/// # When to use:
///
/// - Comparing JSON responses, XML, HTML, or other text formats
/// - Validating API response bodies
/// - Checking log output or error messages
/// - Any scenario where string content differences matter
#[macro_export]
macro_rules! check_str_eq {
    ($left:expr, $right:expr$(,)?) => ({
        $crate::check_str_eq!(@ $left, $right, "", "");
    });
    ($left:expr, $right:expr, $($arg:tt)*) => ({
        $crate::check_str_eq!(@ $left, $right, ": ", $($arg)+);
    });
    (@ $left:expr, $right:expr, $maybe_colon:expr, $($arg:tt)*) => ({
        match (&($left), &($right)) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let __message = format!("check failed: `(left == right)`{}{}\
                       \n\
                       \n{}\
                       \n",
                       $maybe_colon,
                       format_args!($($arg)*),
                       tanu::pretty_assertions::StrComparison::new(left_val, right_val)
                    );
                    let __check = tanu::runner::Check::error(&__message);
                    tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
                    Err(Error::StrEq(__message))?;
                } else {
                    let __message = format!("check succeeded: `(left == right)`{}{}\
                       \n\
                       \n{}\
                       \n",
                       $maybe_colon,
                       format_args!($($arg)*),
                       tanu::pretty_assertions::StrComparison::new(left_val, right_val)
                    );
                    let __check = tanu::runner::Check::success(&__message);
                    tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
                }
            }
        }
    });
}

/// Asserts that two expressions are equal using `==`.
///
/// This macro provides enhanced equality checking with detailed diff output
/// when assertions fail. It integrates with tanu's test reporting system
/// and provides clear, colored output showing the differences between values.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu::{check_eq, http::Client};
///
/// #[tanu::test]
/// async fn test_response_data() -> eyre::Result<()> {
///     let client = Client::new();
///     let response = client.get("https://httpbin.org/json").send().await?;
///     
///     // Assert status code
///     check_eq!(200, response.status().as_u16());
///     
///     // Parse and assert JSON values
///     let data: serde_json::Value = response.json().await?;
///     check_eq!("Wake up to WonderWidgets!", data["slideshow"]["title"]);
///     
///     // With custom message
///     check_eq!(expected_count, actual_count, "User count mismatch");
///     
///     Ok(())
/// }
/// ```
///
/// # Behavior
///
/// - On success: Publishes a success event to the test runner
/// - On failure: Shows a detailed diff highlighting differences and returns an `eyre::Error`
/// - Works with any type that implements `Debug` and `PartialEq`
/// - Provides colored output for better readability
#[macro_export]
macro_rules! check_eq {
    ($left:expr, $right:expr$(,)?) => ({
        $crate::check_eq!(@ $left, $right, "", "");
    });
    ($left:expr, $right:expr, $($arg:tt)*) => ({
        $crate::check_eq!(@ $left, $right, ": ", $($arg)+);
    });
    (@ $left:expr, $right:expr, $maybe_colon:expr, $($arg:tt)*) => ({
        match (&($left), &($right)) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let __message = format!("check failed: `(left == right)`{}{}\
                       \n\
                       \n{}\
                       \n",
                       $maybe_colon,
                       format_args!($($arg)*),
                       tanu::pretty_assertions::Comparison::new(left_val, right_val)
                    );
                    let __check = tanu::runner::Check::error(&__message);
                    tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
                    Err(tanu::assertion::Error::Eq(__message))?;
                } else {
                    let __message = format!("check succeeded: `(left == right)`{}{}\
                       \n\
                       \n{}\
                       \n",
                       $maybe_colon,
                       format_args!($($arg)*),
                       tanu::pretty_assertions::Comparison::new(left_val, right_val)
                    );
                    let __check = tanu::runner::Check::success(&__message);
                    tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
                }
            }
        }
    });
}

/// Asserts that two expressions are not equal using `!=`.
///
/// This macro verifies that two values are different and provides detailed
/// output when the assertion fails (i.e., when the values are unexpectedly equal).
/// It integrates with tanu's test reporting system for consistent error handling.
///
/// # Examples
///
/// ```rust,ignore
/// use tanu::{check_ne, http::Client};
///
/// #[tanu::test]
/// async fn test_different_responses() -> eyre::Result<()> {
///     let client = Client::new();
///     
///     let response1 = client.get("https://httpbin.org/uuid").send().await?;
///     let uuid1: serde_json::Value = response1.json().await?;
///     
///     let response2 = client.get("https://httpbin.org/uuid").send().await?;
///     let uuid2: serde_json::Value = response2.json().await?;
///     
///     // Assert that two UUID responses are different
///     check_ne!(uuid1["uuid"], uuid2["uuid"]);
///     
///     // Assert non-error status codes
///     check_ne!(500, response1.status().as_u16(), "Server should not return 500");
///     
///     Ok(())
/// }
/// ```
///
/// # Common Use Cases:
///
/// - **Unique identifiers**: Ensuring IDs, UUIDs, or tokens are different
/// - **Error conditions**: Verifying responses are not error codes
/// - **State changes**: Confirming values have changed after operations
/// - **Cache validation**: Ensuring content is different after cache invalidation
///
/// # Behavior
///
/// - On success (values are different): Publishes a success event
/// - On failure (values are equal): Shows both values and returns an `eyre::Error`
/// - Works with any type that implements `Debug` and `PartialEq`
#[macro_export]
macro_rules! check_ne {
    ($left:expr, $right:expr$(,)?) => ({
        $crate::check_ne!(@ $left, $right, "", "");
    });
    ($left:expr, $right:expr, $($arg:tt)+) => ({
        $crate::check_ne!(@ $left, $right, ": ", $($arg)+);
    });
    (@ $left:expr, $right:expr, $maybe_colon:expr, $($arg:tt)+) => ({
        match (&($left), &($right)) {
            (left_val, right_val) => {
                if *left_val == *right_val {
                    let __message = format!("check failed: `(left != right)`{}{}\
                        \n\
                        \nBoth sides:\
                        \n{:#?}\
                        \n\
                        \n",
                        $maybe_colon,
                        format_args!($($arg)+),
                        left_val
                    );
                    let __check = tanu::runner::Check::error(&__message);
                    tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
                    Err(Error::Ne(__message))?;
                } else {
                    let __message = format!("check succeeded: `(left != right)`{}{}\
                        \n\
                        \nBoth sides:\
                        \n{:#?}\
                        \n\
                        \n",
                        $maybe_colon,
                        format_args!($($arg)+),
                        left_val
                    );
                    let __check = tanu::runner::Check::success(&__message);
                    tanu::runner::publish(tanu::runner::EventBody::Check(Box::new(__check)))?;
                }
            }
        }
    });
}
