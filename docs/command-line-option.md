# Command Line Options

!!! tip
    Many command-line options can be configured as defaults in `tanu.toml` under the `[runner]` section. See the [Configuration](configuration.md#runner) page for details. Command-line flags always override configuration file settings.

## `test`
Run tests with tanu.

### Options
* `--capture-http`         Capture http debug logs. Can also be set in `tanu.toml` as `runner.capture_http = true`.
* `--show-sensitive`       Show sensitive data (API keys, tokens) in HTTP logs instead of masking them. By default, sensitive query parameters (api_key, access_token, token, secret, password) and headers (authorization, x-api-key, cookie) are masked with `*****` for security. Use this flag to display actual values during debugging. Can also be set in `tanu.toml` as `runner.show_sensitive = true`.
* `--capture-rust`         Capture Rust "log" crate based logs. This is usefull in the following two cases 1) tanu failed unexpectedly and you would want to see the tanu's internal logs. 2) you would want to see logs produced from your tests that uses "log" crate. Can also be set in `tanu.toml` as `runner.capture_rust = true`.
* `-p, --projects <PROJECTS>`  Run only the specified projects. This option can be specified multiple times e.g. --projects dev --projects staging
* `-m, --modules <MODULES>`    Run only the specified modules. This option can be specified multiple times e.g. --modules foo --modules bar
* `-t, --tests <TESTS>`        Run only the specified test cases. This option can be specified multiple times e.g. --tests a ---tests b
* `--reporter <REPORTER>`  Specify the reporter to use. Default is "table". Possible values are "table", "list" and "null"
* `-c, --concurrency <NUMBER>` Specify the maximum number of tests to run in parallel. When unspecified, all tests run in parallel. Can also be set in `tanu.toml` as `runner.concurrency = 4`.
* `--color <WHEN>`         Control when colored output is used. Possible values are "auto" (default), "always", or "never". Environment variable `CARGO_TERM_COLOR` is also respected.

## `tui`
Launch the TUI (Text User Interface) for tanu.

### Options
* `--log-level <LOG_LEVEL>`            [default: Info]
* `--tanu-log-level <TANU_LOG_LEVEL>`  [default: Info]
* `-c, --concurrency <NUMBER>` Specify the maximum number of tests to run in parallel. Default is the number of logical CPU cores. Can also be set in `tanu.toml` as `runner.concurrency = 4`.

## `ls`
List test cases.

## `help`
Print this message or the help of the given subcommand(s).

## Options
* `-h, --help`
Print help.
* `-V, --version`
Print version.
