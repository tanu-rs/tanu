# Configuration

tanu reads its configuration from `tanu.toml` in the current directory, or from the path in the `TANU_CONFIG` environment variable. The file is optional: without it, tanu runs every test in a single project named `default`.

A `tanu.toml` has three kinds of sections:

| Section | Purpose |
|---|---|
| `[[projects]]` | One entry per environment (dev, staging, production, ...). Tests run once per project. |
| `[runner]` | Global defaults for test execution, such as HTTP capture and concurrency. |
| `[tui]` | Appearance of the TUI. |

## Example

```toml
[tui]
payload.color_theme = "tomorrow-night"

[runner]
capture_http = "on-failure"
concurrency = 4

[[projects]]
name = "staging"
base_url = "https://staging.api.example.com"
test_ignore = [
  "example::feature_flag::feature_flag_enabled",
  "example::feature_flag::feature_flag_disabled",
]
retry.count = 3
retry.factor = 2.0
retry.jitter = true
retry.min_delay = "1s"
retry.max_delay = "60s"

[[projects]]
name = "production"
base_url = "https://api.example.com"
test_only = [
  "example::health::health_check",
]
```

## Projects

Inspired by Playwright, `[[projects]]` let you run the same set of tests against different environments or settings. You can define as many projects as you like; use `--projects` on the command line to pick a subset.

Each project supports the following keys. Any other key is a [user-defined setting](#user-defined-settings).

| Key | Description |
|---|---|
| `name` | **Required.** Project name shown in test output and used by `--projects`. |
| `test_ignore` | Tests to skip in this project. |
| `test_only` | If non-empty, run only these tests in this project. An empty or omitted list runs all tests. |
| `retry.*` | Retry policy for failed tests. See [Retry](#retry). |

`test_ignore` and `test_only` take full test names: `<crate>::<module path>::<function>`, for example `example::health::health_check`. Parameterized cases include the case name, such as `example::status::status_codes::404`. Run `cargo run -- ls` to list the exact names. When both lists are set, a test runs only if it is listed in `test_only` and not listed in `test_ignore`.

## Runner

The `[runner]` section configures global test execution behavior. All values are optional and serve as defaults that can be overridden by command-line flags.

```toml
[runner]
capture_http = "on-failure" # "all", "on-failure" (default), or "off"; true/false also accepted
capture_rust = false        # Capture Rust "log" crate logs (default: false)
show_sensitive = false      # Show sensitive data in HTTP logs (default: false)
max_body_size = "64KB"      # Max bytes of an HTTP body printed in logs (default: "64KB", 0 disables)
concurrency = 4             # Max parallel tests (default: unlimited for CLI, CPU cores for TUI)
fail_fast = false           # Abort after the first failure (default: false)
extra_sensitive_keys = ["my_company_token", "internal_secret"]      # Extra field/param substrings to mask
extra_sensitive_headers = ["x-my-custom-auth", "x-internal-token"]  # Extra headers to mask
```

### Options

- `capture_http`: Controls when HTTP request/response logs are captured and displayed. Accepts a boolean (`true` = `"all"`, `false` = `"off"`) or a string (`"all"`, `"on-failure"`, or `"off"`). Default is `"on-failure"` (show HTTP logs only for failed tests). Use `"all"` to show logs for every test, or `"off"` to suppress logs entirely. Can be overridden with `--capture-http[=MODE]` on the command line.
- `capture_rust`: When enabled, captures logs from Rust's `log` crate. Useful for debugging tanu internals or test code that uses the log crate. Default is `false`. Can be overridden with `--capture-rust`.
- `show_sensitive`: When enabled, displays sensitive data (API keys, tokens, passwords) in HTTP logs instead of masking them with `*****`. Use with caution as this may expose secrets. Default is `false`. Can be overridden with `--show-sensitive`.
- `max_body_size`: Caps how many bytes of each HTTP request/response body are printed in logs. Accepts a byte count (`65536`) or a size string (`"64KB"`, `"1.5MB"`); unit suffixes are case-insensitive binary multiples, so `KB` means 1024 bytes and `KB`/`KiB` are equivalent. Set `0`, `"unlimited"`, `"none"`, or `"off"` to print bodies in full. Default is `"64KB"`. A body over the cap is printed as plain truncated text followed by a marker line, without JSON pretty-printing or syntax highlighting. Applies to both `--capture-http` output and the TUI payload view; can be overridden with `--max-body-size` in CLI mode only, since the TUI reads this value from `tanu.toml`.
- `concurrency`: Maximum number of tests to run in parallel. If not specified, CLI mode runs all tests in parallel (unlimited), while TUI mode defaults to the number of CPU cores. Can be overridden with `-c` or `--concurrency`.
- `fail_fast`: When enabled, aborts test execution after the first failure. Remaining tests are skipped and counted as skipped in the summary. Default is `false`. Can be overridden with `--fail-fast`.
- `extra_sensitive_keys`: A list of additional substrings to treat as sensitive in query parameters, URL params, and request/response body fields. Matching is case-insensitive and uses substring logic — an entry of `"company_token"` will mask any field whose name contains `company_token`. Adds to the built-in list; does not replace it.
- `extra_sensitive_headers`: A list of additional HTTP header names (exact match, case-insensitive) to mask in both request and response logs. Adds to the built-in list; does not replace it.

### Credential masking

By default, tanu masks sensitive data in all HTTP logs (both `--capture-http` output and the TUI payload view). Masking is applied to:

- **Request and response headers** — values are replaced with `*****` for headers including `authorization`, `x-api-key`, `x-auth-token`, `cookie`, `set-cookie`, `proxy-authorization`, `x-amz-security-token`, `x-csrf-token`, `x-csrf`, and `api-key`.
- **URL query parameters** — values are masked for any parameter whose name contains a sensitive keyword (e.g. a param named `client_secret` is masked because it contains `secret`).
- **Request and response bodies** — for `application/json` and `application/x-www-form-urlencoded` content types, field values are masked recursively when the field name matches a sensitive keyword.

Built-in sensitive keywords (substring-matched, case-insensitive): `token`, `secret`, `password`, `passwd`, `pwd`, `key`, `auth`, `credential`, `credentials`, `private_key`, `session`, `session_id`, `id_token`, `signature`, `client_id`, `access_token`, `api_key`, `apikey`.

!!! warning
    Substring matching intentionally errs on the side of over-masking. Fields like `token_type` or `public_key` will be masked because they contain `token` / `key`. Use `show_sensitive = true` when you need to inspect such values during debugging.

!!! note
    Use `extra_sensitive_keys` and `extra_sensitive_headers` to mask project-specific secrets beyond the built-in list — for example, internal header names or proprietary credential field names used by your API.

!!! note
    Command-line flags always take precedence over configuration file settings. Use configuration file settings to establish project defaults and command-line flags for one-off overrides.

## Retry

Retries re-run a **whole test** when it returns an error (including a failed `check!`), using exponential backoff. Retries are configured per project and are disabled by default. Individual HTTP requests are not retried on their own.

| Key | Default | Description |
|---|---|---|
| `retry.count` | `0` | Number of retry attempts after the first failure. |
| `retry.factor` | `2.0` | Backoff multiplier between attempts. |
| `retry.jitter` | `false` | Add random jitter to the backoff delay. |
| `retry.min_delay` | `"1s"` | Initial delay. Accepts human-readable durations such as `"100ms"` or `"2s"`. |
| `retry.max_delay` | `"60s"` | Upper bound for the delay. |

```toml
[[projects]]
name = "staging"
retry.count = 3
retry.min_delay = "500ms"
```

In CLI output, failed attempts that will be retried are marked `retrying...`.

## User-defined settings

Any key in a `[[projects]]` entry other than the ones above is stored as a user-defined setting. This is the idiomatic place for values that differ between environments, like base URLs:

```toml
[[projects]]
name = "staging"
base_url = "https://api.staging.example.com"

[[projects]]
name = "production"
base_url = "https://api.example.com"
```

Read a value for the currently running project with `tanu::get_config()`:

```rust
use tanu::{check, eyre, http::Client};

#[tanu::test]
async fn health() -> eyre::Result<()> {
    let base_url = tanu::get_config().get_str("base_url")?.to_string();
    let res = Client::new().get(format!("{base_url}/health")).send().await?;
    check!(res.status().is_success());
    Ok(())
}
```

`get_config()` returns the configuration of the project the test is running in, so the same test hits a different server in each project.

Other accessors:

| Method | Returns |
|---|---|
| [`get`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get) | The raw `toml::Value` |
| [`get_str`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_str) | `&str` |
| [`get_int`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_int) | `i64` |
| [`get_float`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_float) | `f64` |
| [`get_bool`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_bool) | `bool` |
| [`get_datetime`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_datetime) | `DateTime<Utc>` |
| [`get_array`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_array) | `Vec<T>` deserialized from a JSON string |
| [`get_object`](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_object) | `T` deserialized from a JSON string |

!!! note
    The typed accessors (`get_int`, `get_float`, `get_bool`, `get_datetime`, `get_array`, `get_object`) parse **string** values, because values from environment variables are always strings. In `tanu.toml`, write these values as quoted strings (`timeout = "5000"`, `ids = "[1, 2, 3]"`), or read native TOML values with `get`.

!!! tip "Accessing config from spawned tasks"
    `get_config()` relies on Tokio task-local storage. If you call it inside `tokio::spawn`, wrap the future with `tanu::scope_current(...)`. See the [FAQ](faq.md#i-see-a-panic-cannot-access-a-task-local-storage-value-without-setting-it-first).

## Environment variables

### Config file location

By default, tanu reads `tanu.toml` from the current directory. Set `TANU_CONFIG` to use a different file:

```bash
TANU_CONFIG=./config/tanu.staging.toml cargo run -- test
```

If `TANU_CONFIG` points to a file that doesn't exist, tanu exits with an error.

!!! warning
    `TANU_CONFIG` is reserved for the config file path. Don't use it as a config value (e.g. `TANU_CONFIG=true`); tanu reports an error if it detects this.

### User-defined values from the environment

Secrets such as API keys shouldn't be committed to `tanu.toml`. Provide them through environment variables instead. tanu also loads a `.env` file from the current directory if one exists.

**Global values:** a variable named `TANU_<KEY>` is available in every project as `<key>` (lowercased).

```bash
# get_config().get_str("api_key") in every project
export TANU_API_KEY=secret123
```

**Project values:** a variable named `TANU_<PROJECT>_<KEY>` is available only in that project. For example, `TANU_STAGING_API_KEY` is readable as `get_config().get_str("api_key")` while running the `staging` project.

```bash
# .env
TANU_STAGING_API_KEY=staging-secret
TANU_PRODUCTION_API_KEY=production-secret
```

Environment values override keys of the same name in `tanu.toml`.

## TUI theme

The `[tui]` section customizes the TUI. `payload.color_theme` sets the color theme used to syntax-highlight request and response payloads (particularly JSON) in the Payload tab:

```toml
[tui]
payload.color_theme = "tomorrow-night"
```

tanu ships with the full set of Base16 themes:

`3024` · `apathy` · `ashes` · `atelier-cave` · `atelier-dune` · `atelier-estuary` · `atelier-forest` · `atelier-heath` · `atelier-lakeside` · `atelier-plateau` · `atelier-savanna` · `atelier-seaside` · `atelier-sulphurpool` · `atlas` · `bespin` · `black-metal` · `brewer` · `bright` · `brushtrees` · `chalk` · `circus` · `classic` · `codeschool` · `cupcake` · `cupertino` · `darktooth` · `default` · `eighties` · `embers` · `flat` · `fruit-soda` · `github` · `google` · `grayscale` · `greenscreen` · `gruvbox` · `harmonic` · `hopscotch` · `irblack` · `isotope` · `macintosh` · `marrakesh` · `materia` · `material` · `mellow` · `mexico` · `mocha` · `monokai` · `nord` · `ocean` · `oceanicnext` · `one` · `onedark` · `papercolor` · `paraiso` · `phd` · `pico` · `pop` · `porple` · `railscasts` · `rebecca` · `seti` · `shapeshifter` · `solarflare` · `solarized` · `spacemacs` · `summerfruit` · `tomorrow` · `tomorrow-night` · `tube` · `twilight` · `unikitty` · `woodland` · `xcode` · `zenburn`
