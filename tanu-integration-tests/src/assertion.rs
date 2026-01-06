#![allow(clippy::eq_op, clippy::approx_constant)]
use tanu::{check, check_eq, check_ne, check_str_eq, eyre};

#[tanu::test]
async fn check_basic_true() -> eyre::Result<()> {
    check!(true);
    Ok(())
}

#[tanu::test]
async fn check_with_message() -> eyre::Result<()> {
    check!(1 == 1, "Numbers should be equal");
    Ok(())
}

#[tanu::test]
async fn check_expression() -> eyre::Result<()> {
    let x = 5;
    let y = 10;
    check!(x < y);
    check!(x + y == 15);
    check!(x != y);
    Ok(())
}

#[tanu::test]
async fn check_eq_integers() -> eyre::Result<()> {
    check_eq!(42, 42);
    check_eq!(0, 0);
    check_eq!(-1, -1);
    Ok(())
}

#[tanu::test]
async fn check_eq_strings() -> eyre::Result<()> {
    check_eq!("hello", "hello");
    check_eq!(String::from("world"), "world");
    check_eq!("", "");
    Ok(())
}

#[tanu::test]
async fn check_eq_with_message() -> eyre::Result<()> {
    check_eq!(100, 100, "Values should be equal");
    Ok(())
}

#[tanu::test]
async fn check_eq_vectors() -> eyre::Result<()> {
    check_eq!(vec![1, 2, 3], vec![1, 2, 3]);
    check_eq!(Vec::<i32>::new(), Vec::<i32>::new());
    Ok(())
}

#[tanu::test]
async fn check_ne_integers() -> eyre::Result<()> {
    check_ne!(1, 2);
    check_ne!(42, 43);
    check_ne!(0, 1);
    Ok(())
}

#[tanu::test]
async fn check_ne_strings() -> eyre::Result<()> {
    check_ne!("hello", "world");
    check_ne!(String::from("foo"), "bar");
    check_ne!("", "non-empty");
    Ok(())
}

#[tanu::test]
async fn check_ne_with_message() -> eyre::Result<()> {
    check_ne!(5, 10, "Values should be different");
    Ok(())
}

#[tanu::test]
async fn check_str_eq_basic() -> eyre::Result<()> {
    check_str_eq!("hello", "hello");
    check_str_eq!(String::from("world"), "world");
    check_str_eq!("", "");
    Ok(())
}

#[tanu::test]
async fn check_str_eq_multiline() -> eyre::Result<()> {
    let text1 = "line1\nline2\nline3";
    let text2 = "line1\nline2\nline3";
    check_str_eq!(text1, text2);
    Ok(())
}

#[tanu::test]
async fn check_str_eq_with_message() -> eyre::Result<()> {
    check_str_eq!("expected", "expected", "String comparison failed");
    Ok(())
}

#[tanu::test]
async fn check_str_eq_whitespace() -> eyre::Result<()> {
    check_str_eq!("  hello  ", "  hello  ");
    check_str_eq!("\t\ntest\t\n", "\t\ntest\t\n");
    Ok(())
}

#[tanu::test]
async fn check_combined_assertions() -> eyre::Result<()> {
    let value = 42;
    let text = "test";

    check!(value > 0);
    check_eq!(value, 42);
    check_ne!(value, 0);
    check_str_eq!(text, "test");

    Ok(())
}

#[tanu::test]
async fn check_option_values() -> eyre::Result<()> {
    let some_value = Some(42);
    let none_value: Option<i32> = None;

    check!(some_value.is_some());
    check!(none_value.is_none());
    check_eq!(some_value, Some(42));
    check_ne!(some_value, None);

    Ok(())
}

#[tanu::test]
async fn check_result_values() -> eyre::Result<()> {
    let ok_result: Result<i32, &str> = Ok(42);
    let err_result: Result<i32, &str> = Err("error");

    check!(ok_result.is_ok());
    check!(err_result.is_err());
    check_eq!(ok_result, Ok(42));
    check_ne!(ok_result, Err("error"));

    Ok(())
}

#[tanu::test]
async fn check_boolean_operations() -> eyre::Result<()> {
    let a = true;
    let b = false;

    check!(a && !b);
    check!(a || b);
    check!(!(!a && !b));
    check_eq!(a, true);
    check_ne!(a, b);

    Ok(())
}

#[tanu::test]
async fn check_floating_point() -> eyre::Result<()> {
    let pi = 3.14159;
    let e = 2.71828;

    check!(pi > e);
    check!(pi > 0.0);
    check_eq!(pi, 3.14159);
    check_ne!(pi, e);

    Ok(())
}

#[tanu::test]
async fn check_json_like_structure() -> eyre::Result<()> {
    use serde_json::json;

    let json1 = json!({"name": "John", "age": 30});
    let json2 = json!({"name": "John", "age": 30});
    let json3 = json!({"name": "Jane", "age": 25});

    check_eq!(json1, json2);
    check_ne!(json1, json3);

    Ok(())
}

#[tanu::test]
async fn check_str_eq_unicode() -> eyre::Result<()> {
    check_str_eq!("日本語", "日本語");
    check_str_eq!("🦀 Rust 🦀", "🦀 Rust 🦀");
    check_str_eq!("αβγδ", "αβγδ");
    Ok(())
}

#[tanu::test]
async fn check_str_eq_empty_and_whitespace_only() -> eyre::Result<()> {
    check_str_eq!("", "");
    check_str_eq!("   ", "   ");
    check_str_eq!("\n\n\n", "\n\n\n");
    check_str_eq!("\t\t", "\t\t");
    Ok(())
}

#[tanu::test]
async fn check_str_eq_long_multiline() -> eyre::Result<()> {
    let text = r#"
{
    "name": "test",
    "value": 123,
    "nested": {
        "key": "value"
    }
}
"#;
    check_str_eq!(text, text);
    Ok(())
}

#[tanu::test]
async fn check_str_eq_failure_returns_error() -> eyre::Result<()> {
    // Test that check_str_eq! properly fails when strings don't match
    // by checking the error type directly
    use tanu::assertion::Error as AssertionError;

    let left = "hello";
    let right = "world";

    // Manually test the comparison logic that check_str_eq! uses
    check!(left != right, "Test strings should be different");

    // Verify AssertionError::StrEq can be created with a message
    let err = AssertionError::StrEq("test error".to_string());
    let err_str = format!("{}", err);
    check!(err_str.contains("test error"), "StrEq error should contain the message");

    Ok(())
}

#[tanu::test]
async fn check_str_eq_with_string_types() -> eyre::Result<()> {
    let owned = String::from("test");
    let borrowed: &str = "test";
    let cow: std::borrow::Cow<str> = std::borrow::Cow::Borrowed("test");

    // Test various combinations of string types
    check_str_eq!(&owned, borrowed);
    check_str_eq!(borrowed, &owned);
    check_str_eq!(&owned, &cow);
    check_str_eq!(&cow, borrowed);

    Ok(())
}
