# tanu.toml Configuration

The `tanu.toml` file is used to configure different project environments for the tanu application.

## Structure

The [[projects]] tables in the tanu.toml file allow you to define different configurations for various environments. This is inspired by Playwright and enables you to iterate the same set of tests with different configurations or environments. You can make as many projects as you want.

The `tanu.toml` file consists of multiple `[[projects]]` tables, each representing a different environment. Each table contains the following fields:

- `name`: The name of the project (e.g., "dev", "staging", "production").
- `test_ignore`: A list of test cases to ignore for the environment.

## Example

Below is an example of a `tanu.toml` file:

```toml
[tanu]
payload.color_theme: "tomorrow-night"  # Replace with your preferred theme name

[[projects]]
name = "staging"
test_ignore = [
  "feature_flag::feature_flag_enabled",
  "feature_flag::feature_flag_disabled",
]
retry.count = 3
retry.factor = 2.0
retry.jitter = true
retry.min_delay = "1s"
retry.max_delay = "60s"

[[projects]]
name = "production"
test_ignore = []
retry.count = 3
retry.factor = 2.0
retry.jitter = true
retry.min_delay = "1s"
retry.max_delay = "60s"
```

## Retry

This section describes the HTTP retry configuration for the project. All values are optional. If the retry configuration is entirely omitted, retries are disabled by default. If configured, the Tanu runner will perform retry attempts if a request fails.
- `retry.count`: The number of retry attempts. Default is 0.
- `retry.factor`: The factor for exponential backoff. Default is 2.0.
- `retry.jitter`: A boolean to enable or disable backoff jitter. Default is false.
- `retry.min_delay`: The minimum delay for backoff. Default is "1s".
- `retry.max_delay`: The maximum delay for backoff. Default is "60s".

## User defined settings

tanu allows you to set user-defined settings in `tanu.toml`. You can set arbitrary key-value pairs under each project setting.

Here is an example specifying different `base_url` values for staging and production environments:

```toml
[[projects]]
name = "staging"
base_url = "https://api.production.foobar.com"

[[projects]]
name = "production"
base_url = "https://api.staging.foobar.com"
```

In your test code, you can retrieve the value for the current project using the following method:
```rust
tanu::get_config().get_str("base_url")?;
```

If the value is not string, you can use other methods to retrieve it:

- [get_int](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_int)
- [get_float](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_float)
- [get_bool](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_bool)
- [get_datetime](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_datetime)
- [get_array](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_array)
- [get_object](https://docs.rs/tanu/latest/tanu/struct.ProjectConfig.html#method.get_object)

## Environment variables

Tanu also allows you to set user-defined settings in a `.env` file. Secret settings like API keys should not be stored in plain text; instead, environment variables should be used. Any environment variable prefixed with `TANU_{PROJECT}_{NAME}` will be exposed as a configuration. For example, an API key set in the `TANU_STAGING_API_KEY` environment variable can be accessed using `tanu::get_config().get_str("api_key")`.

## Theme

You can customize the appearance of Tanu's interface by selecting a color theme.

To change the theme, add the following to your `tanu.toml` configuration file:

```toml
[tanu]
payload.color_theme = "tomorrow-night"  # Replace with your preferred theme name
```

!!! note
    The color theme setting primarily affects the Payload tab in the TUI, where it's used to colorize and syntax-highlight response payloads (particularly JSON responses). This makes the API responses more readable and helps you quickly identify different elements in the response data.

### Available Themes

Tanu ships with all Base16 themes, providing a consistent color palette across different interfaces.
Available themes include:

- `3024`
- `apathy`
- `ashes`
- `atelier-cave`
- `atelier-dune`
- `atelier-estuary`
- `atelier-forest`
- `atelier-heath`
- `atelier-lakeside`
- `atelier-plateau`
- `atelier-savanna`
- `atelier-seaside`
- `atelier-sulphurpool`
- `atlas`
- `bespin`
- `black-metal`
- `brewer`
- `bright`
- `brushtrees`
- `chalk`
- `circus`
- `classic`
- `codeschool`
- `cupcake`
- `cupertino`
- `darktooth`
- `default`
- `eighties`
- `embers`
- `flat`
- `fruit-soda`
- `github`
- `google`
- `grayscale`
- `greenscreen`
- `gruvbox`
- `harmonic`
- `hopscotch`
- `irblack`
- `isotope`
- `macintosh`
- `marrakesh`
- `materia`
- `material`
- `mellow`
- `mexico`
- `mocha`
- `monokai`
- `nord`
- `ocean`
- `oceanicnext`
- `one`
- `onedark`
- `papercolor`
- `paraiso`
- `phd`
- `pico`
- `pop`
- `porple`
- `railscasts`
- `rebecca`
- `seti`
- `shapeshifter`
- `solarflare`
- `solarized`
- `spacemacs`
- `summerfruit`
- `tomorrow`
- `tomorrow-night`
- `tube`
- `twilight`
- `unikitty`
- `woodland`
- `xcode`
- `zenburn`
