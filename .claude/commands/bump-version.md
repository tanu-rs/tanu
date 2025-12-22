Bump the version to $ARGUMENTS across all crates.

Steps:
1. Update crate versions in:
   - `tanu/Cargo.toml`
   - `tanu-core/Cargo.toml`
   - `tanu-derive/Cargo.toml`
   - `tanu-tui/Cargo.toml`
   - `tanu-integration-tests/Cargo.toml`

2. Update pinned internal dependency versions in:
   - `tanu/Cargo.toml`
   - `tanu-core/Cargo.toml`
   - `tanu-tui/Cargo.toml`

3. Update README examples in:
   - `tanu/README.md`
   - `tanu-core/README.md`
   - `tanu-derive/README.md`

4. Verify no stale references remain by running: `rg "<old_version>"`

5. Create a commit with message: `🔖 Bump version to v$ARGUMENTS`
