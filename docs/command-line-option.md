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
cargo run -- test [OPTIONS] [PATTERN]...
```

### Filtering

| Option | Description |
|---|---|
| `[PATTERN]...` | Run only tests whose full name (`module::test`) contains one of these substrings, like `cargo test <PATTERN>`. |
| `-p, --projects <PROJECTS>` | Run only the given projects. |
| `-m, --modules <MODULES>` | Run only tests in the given modules and their submodules. Any run of whole path segments works, so the crate prefix can be left out: `-m http` selects `example::http` and `example::http::cookie`. |
| `-t, --tests <TESTS>` | Run only the given tests, by full name (`example::http::get`) or any trailing part of it made of whole segments (`http::get`, or just `get`, which selects every test named `get`). |

Each option accepts a comma-separated list and can be repeated: `-t a,b` is the same as `-t a -t b`. Filters combine, so `-p staging -m example::http` runs the `example::http` tests in the `staging` project only. Run `cargo run -- ls` to see test names.

A `-p`, `-m`, or `-t` value that matches nothing is an error, with a suggestion when a name is close, so typos don't silently run zero tests:

```text
$ cargo run -- test -t set_cokie
error: no test named "set_cokie"

  tip: did you mean "example::http::cookie::set_cookie"?
```

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
| `-c, --concurrency <N>` | Maximum number of tests running in parallel (at least 1). Unlimited when unspecified. |
| `--fail-fast` | Stop after the first failure. Remaining tests are reported as skipped. |

### Output

| Option | Description |
|---|---|
| `--reporters <REPORTERS>` | Comma-separated reporters to use. Default `live` on an interactive terminal, `list` otherwise (CI, pipes, files). Reporters registered with `App::install_reporter` are also available here; see [Reporters](report.md). |
| `--color <WHEN>` | `auto` (default), `always`, or `never`. The `CARGO_TERM_COLOR` environment variable is also respected. |
| `--capture-rust` | Print logs emitted through the Rust [`log`](https://crates.io/crates/log) crate, both from tanu internals and from your tests. |

### Output

Each test prints one line as it finishes. Failed tests show their error indented beneath, retried tests show which attempt comes next and how many retries they needed, and the run ends with a recap of all failures followed by the counts:

```text
✓ 1 [default] example::http::get (45.20ms)
✘ 2 [default] example::users::create: retrying (attempt 2)...
    status code mismatch: expected 201, got 500
✘ 2 [default] example::users::create (310.12ms) (after 1 retry):
    status code mismatch: expected 201, got 500

Failures:
  ✘ [default] example::users::create
    status code mismatch: expected 201, got 500

Tests: 1 passed, 1 failed, 2 total, 1 retried
Time: 356.81ms (prep: 1.02ms)
```

### Examples

```bash
# Run everything against staging, printing HTTP logs for failures (the default)
cargo run -- test -p staging

# Run every test whose name contains "users"
cargo run -- test users

# Debug one test with full HTTP output
cargo run -- test -t example::users::create_user --capture-http

# CI: limit parallelism and stop at the first failure
cargo run -- test -c 4 --fail-fast --color always
```

## `tui`

Launch the interactive [terminal UI](tui.md).

| Option | Description |
|---|---|
| `-c, --concurrency <N>` | Maximum number of tests running in parallel (at least 1). Default: number of logical CPU cores. |
| `--log-level <LEVEL>` | Log level for the logger pane. Default `Debug`. |
| `--tanu-log-level <LEVEL>` | Log level for tanu's internal logs. Default `Debug`. |

## `ls`

List discovered tests, grouped by module, once per project:

```text
* example::http
  - [default] example::http::get
* example::parameterized
  - [default] example::parameterized::add::10_10_20
  - [default] example::parameterized::add::20_20_40
```

`ls` accepts the same filters as `test` (`[PATTERN]...`, `-p`, `-m`, `-t`), so you can check what a filter selects before running it: `cargo run -- ls -m http`.

## Global options

| Option | Description |
|---|---|
| `-h, --help` | Print help. Works on subcommands too, e.g. `test --help`. |
| `-V, --version` | Print version. |
