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
