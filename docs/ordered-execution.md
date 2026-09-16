# Ordered Test Execution

Ordered tests run one after another, in source order, within a module. Use them for flows where each step builds on the previous one, like create → update → verify → delete, while unrelated tests keep running in parallel.

At a glance:

- Tests in an ordered module run **sequentially**.
- Order follows **source line numbers** (top to bottom in the file).
- Different ordered modules run **in parallel** with each other and with regular tests.
- Ordering is **project-scoped**: the same module running in two projects doesn't block itself.

## Ordering a module

Apply `#[tanu::test(ordered)]` to a module. Every `#[tanu::test]` function directly inside it runs in source order:

```rust
#[tanu::test(ordered)]
mod user_lifecycle {
    use tanu::eyre;

    #[tanu::test]
    async fn step_1_create() -> eyre::Result<()> {
        Ok(())
    }

    #[tanu::test]
    async fn step_2_update() -> eyre::Result<()> {
        Ok(())
    }

    #[tanu::test]
    async fn step_3_verify() -> eyre::Result<()> {
        Ok(())
    }

    #[tanu::test]
    async fn step_4_delete() -> eyre::Result<()> {
        Ok(())
    }
}
```

Moving a function up or down in the file changes when it runs. Function names don't matter; the `step_N` prefixes above are only for readability.

!!! note
    `ordered` is only valid on modules. Using `#[tanu::test]` on a module without `ordered` is a compile error.

## Failures

A failing step doesn't stop the rest of the module: the remaining tests still run, in order, and each is reported individually. If later steps can't succeed without earlier ones, check preconditions at the start of each step so failures point at the root cause.

With `--fail-fast`, no further tests start after the first failure, including the rest of the ordered module.

Retries apply per test. A failing step is retried according to the project's [retry settings](configuration.md#retry) before the next step starts.

## Sharing state between steps

Each test is a separate function, so pass data between steps through shared state, for example a `static` protected by a lock:

```rust
#[tanu::test(ordered)]
mod user_lifecycle {
    use tanu::{check, eyre, http::Client};
    use tokio::sync::Mutex;

    static USER_ID: Mutex<Option<String>> = Mutex::const_new(None);

    #[tanu::test]
    async fn create() -> eyre::Result<()> {
        let id = "42".to_string(); // e.g. parsed from the create response
        *USER_ID.lock().await = Some(id);
        Ok(())
    }

    #[tanu::test]
    async fn fetch() -> eyre::Result<()> {
        let id = USER_ID.lock().await.clone();
        check!(id.is_some(), "create step did not store a user id");
        Ok(())
    }
}
```

Remember that each test runs once per project. Key shared state by project name (`tanu::get_config().name`) if several projects run the module at the same time.

## Interaction with serial groups

`ordered` implies a serial group per module (based on `module_path!()`). If you also specify `serial` on a test inside an ordered module, the ordered grouping takes precedence, so there's no reason to combine them.

## Parallelism and concurrency

Ordered modules only serialize their own tests. Other ordered modules and non-ordered tests keep running in parallel, and every test, ordered or not, still counts against the `--concurrency` limit.
