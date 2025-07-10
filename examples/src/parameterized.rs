use tanu::{check_eq, eyre};

#[tanu::test(10, 10, 20)]
#[tanu::test(20, 20, 40)]
async fn auto_generated_test_name(a: u32, b: u32, expected: u32) -> eyre::Result<()> {
    let result = a + b;
    check_eq!(result, expected);
    Ok(())
}

#[tanu::test(10, 10, 20; "specify_test_name_add_10_and_10_equal_20")]
#[tanu::test(20, 20, 40; "specify_test_name_add_20_and_20_equal_40")]
async fn specify_test_name(a: u32, b: u32, expected: u32) -> eyre::Result<()> {
    let result = a + b;
    check_eq!(result, expected);
    Ok(())
}
