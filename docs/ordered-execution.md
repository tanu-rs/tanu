# Ordered Test Execution

Ordered tests run in source order within a module. This is useful for setup/verify/cleanup
flows where the order of steps matters, while still allowing unrelated tests to run in
parallel.

Key behavior:

- Ordered tests run **sequentially** within the same module.
- Order is based on **source line number** (top to bottom in the file).
- Different ordered modules can run **in parallel**.
- Ordering is **project-scoped** (the same module in different projects does not block).

## Module-level ordering (recommended)

Apply `ordered` to a module and all `#[tanu::test]` inside it will run in source order:

```rust
#[tanu::test(ordered)]
mod setup_tests {
    use tanu::eyre;

    #[tanu::test]
    async fn step_1_init() -> eyre::Result<()> {
        Ok(())
    }

    #[tanu::test]
    async fn step_2_setup() -> eyre::Result<()> {
        Ok(())
    }

    #[tanu::test]
    async fn step_3_verify() -> eyre::Result<()> {
        Ok(())
    }

    #[tanu::test]
    async fn step_4_cleanup() -> eyre::Result<()> {
        Ok(())
    }
}
```

Reordering the functions in the file changes the execution order.

## Interaction with serial groups

`ordered` implies a per-module serial group (based on `module_path!()`), so tests in the
same ordered module are serialized. If you also specify `serial`, the ordered grouping
takes precedence. Prefer not to combine them.

## Parallelism and concurrency

Ordered groups only serialize tests within the same module. Different ordered modules and
non-ordered tests still run in parallel, subject to the configured concurrency limit.
