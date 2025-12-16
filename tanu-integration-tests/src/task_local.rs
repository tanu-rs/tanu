use tanu::{check, eyre};

#[tanu::test]
async fn spawned_task_without_scope_current_panics() -> eyre::Result<()> {
    let _ = tanu::get_config();

    let handle = tokio::spawn(async move {
        let _ = tanu::get_config();
    });

    let join_err = handle
        .await
        .expect_err("spawned task should panic without tanu task-local context");
    check!(join_err.is_panic());

    Ok(())
}

#[tanu::test]
async fn spawned_task_with_scope_current_does_not_panic() -> eyre::Result<()> {
    let handle = tokio::spawn(tanu::scope_current(async move {
        let _ = tanu::get_config();
        check!(true);
        eyre::Ok(())
    }));

    handle
        .await
        .expect("spawned task should not panic")?;

    Ok(())
}
