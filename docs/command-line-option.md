# Command Line Options

## Commands

### `test`
Run tests with tanu.

#### Options
* `--capture-http`         Capture http debug logs
* `--capture-rust`         Capture Rust "log" crate based logs. This is usefull in the following two cases 1) tanu failed unexpectedly and you would want to see the tanu's internal logs. 2) you would want to see logs produced from your tests that uses "log" crate
* `-p, --projects <PROJECTS>`  Run only the specified projects. This option can be specified multiple times e.g. --projects dev --projects staging
* `-m, --modules <MODULES>`    Run only the specified modules. This option can be specified multiple times e.g. --modules foo --modules bar
* `-t, --tests <TESTS>`        Run only the specified test cases. This option can be specified multiple times e.g. --tests a ---tests b
* `--reporter <REPORTER>`  Specify the reporter to use. Default is "table". Possible values are "table", "list" and "null"
* `-c, --concurrency <NUMBER>` Specify the maximum number of tests to run in parallel. When unspecified, all tests run in parallel.
* `--color <WHEN>`         Control when colored output is used. Possible values are "auto" (default), "always", or "never". Environment variable `CARGO_TERM_COLOR` is also respected.

### `tui`
Launch the TUI (Text User Interface) for tanu.

#### Options
* `--log-level <LOG_LEVEL>`            [default: Info]
* `--tanu-log-level <TANU_LOG_LEVEL>`  [default: Info]
* `-c, --concurrency <NUMBER>` Specify the maximum number of tests to run in parallel. Default is the number of logical CPU cores

### `ls`
List test cases.

### `help`
Print this message or the help of the given subcommand(s).

## Options
* `-h, --help`
Print help.
* `-V, --version`
Print version.
