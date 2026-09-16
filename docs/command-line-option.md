# Command Line Options

The binary built with `#[tanu::main]` exposes three subcommands: `test`, `tui`, and `ls`. When running through Cargo, put the subcommand and its flags after `--`:

```bash
cargo run -- test --capture-http -p staging
```

!!! tip
    Many options can be set as defaults in the `[runner]` section of `tanu.toml`. Command-line flags always take precedence. See [Configuration](configuration.md#runner).

## `test`

Run tests in CLI mode and print results to the terminal.

```bash
cargo run -- test [OPTIONS]
```

### Filtering

| Option | Description |
|---|---|
| `-p, --projects <PROJECTS>` | Run only the given projects. |
| `-m, --modules <MODULES>` | Run only tests in the given modules, by full module path (e.g. `example::http`). |
| `-t, --tests <TESTS>` | Run only the given tests, by full name (e.g. `example::http::get`). |

Each filter accepts a comma-separated list and can be repeated: `-t a,b` is the same as `-t a -t b`. Filters combine, so `-p staging -m example::http` runs the `example::http` tests in the `staging` project only. Names must match exactly; run `cargo run -- ls` to see them.

The `test_ignore` and `test_only` lists in `tanu.toml` are applied in addition to these flags.

### HTTP capture

| Option | Description |
|---|---|
| `--capture-http[=MODE]` | When to print captured HTTP requests and responses: `all`, `on-failure` (default), or `off`. A bare `--capture-http` means `all`. |
| `--max-body-size <SIZE>` | Maximum bytes of each request/response body to print. Accepts a byte count (`65536`) or a size (`64KB`, `2MB`, `1.5MB`); `0` or `unlimited` prints bodies in full. Default `16KB`. Truncated bodies are printed as plain text with a marker line, without JSON pretty-printing. |
| `--show-sensitive` | Print credentials in HTTP logs instead of masking them with `*****`. See [credential masking](configuration.md#credential-masking). |

### Execution

| Option | Description |
|---|---|
| `-c, --concurrency <NUMBER>` | Maximum number of tests running in parallel. Unlimited when unspecified. |
| `--fail-fast` | Stop after the first failure. Remaining tests are reported as skipped. |

### Output

| Option | Description |
|---|---|
| `--reporters <REPORTERS>` | Comma-separated reporters to use. Default `list`. Reporters registered with `App::install_reporter` are also available here; see [Reporters](report.md). |
| `--color <WHEN>` | `auto` (default), `always`, or `never`. The `CARGO_TERM_COLOR` environment variable is also respected. |
| `--capture-rust` | Print logs emitted through the Rust [`log`](https://crates.io/crates/log) crate, both from tanu internals and from your tests. |

### Examples

```bash
# Run everything against staging, printing HTTP logs for failures (the default)
cargo run -- test -p staging

# Debug one test with full HTTP output
cargo run -- test -t example::users::create_user --capture-http

# CI: limit parallelism and stop at the first failure
cargo run -- test -c 4 --fail-fast --color always
```

## `tui`

Launch the interactive [terminal UI](tui.md).

| Option | Description |
|---|---|
| `-c, --concurrency <NUMBER>` | Maximum number of tests running in parallel. Default: number of logical CPU cores. |
| `--log-level <LEVEL>` | Log level for the logger pane. Default `Info`. |
| `--tanu-log-level <LEVEL>` | Log level for tanu's internal logs. Default `Info`. |

## `ls`

List all discovered tests, grouped by module, once per project:

```text
* example::http
  - [default] example::http::get
* example::parameterized
  - [default] example::parameterized::add::10_10_20
  - [default] example::parameterized::add::20_20_40
```

## Global options

| Option | Description |
|---|---|
| `-h, --help` | Print help. Works on subcommands too, e.g. `test --help`. |
| `-V, --version` | Print version. |
