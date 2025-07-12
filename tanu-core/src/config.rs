//! # Configuration Module
//!
//! Handles loading and managing tanu configuration from `tanu.toml` files.
//! Supports project-specific configurations, environment variables, and
//! various test execution settings.
//!
//! ## Configuration Structure
//!
//! Tanu uses TOML configuration files with the following structure:
//!
//! ```toml
//! [[projects]]
//! name = "staging"
//! base_url = "https://staging.api.example.com"
//! timeout = 30000
//! retry.count = 3
//! retry.factor = 2.0
//! test_ignore = ["slow_test", "flaky_test"]
//!
//! [[projects]]
//! name = "production"
//! base_url = "https://api.example.com"
//! timeout = 10000
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use tanu::{get_config, get_tanu_config};
//!
//! // Get global configuration
//! let config = get_tanu_config();
//!
//! // Get current project configuration (within test context)
//! let project_config = get_config();
//! let base_url = project_config.get_str("base_url").unwrap_or_default();
//! ```

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize};
use std::{collections::HashMap, io::Read, path::Path, time::Duration};
use toml::Value as TomlValue;
use tracing::*;

use crate::{Error, Result};

static CONFIG: Lazy<Config> = Lazy::new(|| {
    let _ = dotenv::dotenv();
    Config::load().unwrap_or_default()
});

tokio::task_local! {
    pub static PROJECT: ProjectConfig;
}

#[doc(hidden)]
pub fn get_tanu_config() -> &'static Config {
    &CONFIG
}

/// Get configuration for the current project. This function has to be called in the tokio
/// task created by tanu runner. Otherwise, calling this function will panic.
pub fn get_config() -> ProjectConfig {
    PROJECT.get()
}

/// tanu's configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub projects: Vec<ProjectConfig>,
    /// Global tanu configuration
    #[serde(default)]
    pub tui: Tui,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            projects: vec![ProjectConfig {
                name: "default".to_string(),
                ..Default::default()
            }],
            tui: Tui::default(),
        }
    }
}

/// Global tanu configuration
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Tui {
    #[serde(default)]
    pub payload: Payload,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Payload {
    /// Optional color theme for terminal output
    pub color_theme: Option<String>,
}

impl Config {
    /// Load tanu configuration from path.
    fn load_from(path: &Path) -> Result<Config> {
        let Ok(mut file) = std::fs::File::open(path) else {
            return Ok(Config::default());
        };

        let mut buf = String::new();
        file.read_to_string(&mut buf)
            .map_err(|e| Error::LoadError(e.to_string()))?;
        let mut cfg: Config = toml::from_str(&buf).map_err(|e| {
            Error::LoadError(format!(
                "failed to deserialize tanu.toml into tanu::Config: {e}"
            ))
        })?;

        debug!("tanu.toml was successfully loaded: {cfg:#?}");

        cfg.load_env();

        Ok(cfg)
    }

    /// Load tanu configuration from tanu.toml in the current directory.
    fn load() -> Result<Config> {
        Config::load_from(Path::new("tanu.toml"))
    }

    /// Load tanu configuration from environment variables.
    ///
    /// Global environment variables: tanu automatically detects environment variables prefixed
    /// with tanu_XXX and maps them to the corresponding configuration variable as "xxx". This
    /// global configuration can be accessed in any project.
    ///
    /// Project environment variables: tanu automatically detects environment variables prefixed
    /// with tanu_PROJECT_ZZZ_XXX and maps them to the corresponding configuration variable as
    /// "xxx" for project "ZZZ". This configuration is isolated within the project.
    fn load_env(&mut self) {
        static PREFIX: &str = "TANU";

        let global_prefix = format!("{PREFIX}_");
        let project_prefixes: Vec<_> = self
            .projects
            .iter()
            .map(|p| format!("{PREFIX}_{}_", p.name.to_uppercase()))
            .collect();
        debug!("Loading global configuration from env");
        let global_vars: HashMap<_, _> = std::env::vars()
            .filter_map(|(k, v)| {
                let is_project_var = project_prefixes.iter().any(|pp| k.contains(pp));
                if is_project_var {
                    return None;
                }

                k.find(&global_prefix)?;
                Some((
                    k[global_prefix.len()..].to_string().to_lowercase(),
                    TomlValue::String(v),
                ))
            })
            .collect();

        debug!("Loading project configuration from env");
        for project in &mut self.projects {
            let project_prefix = format!("{PREFIX}_{}_", project.name.to_uppercase());
            let vars: HashMap<_, _> = std::env::vars()
                .filter_map(|(k, v)| {
                    k.find(&project_prefix)?;
                    Some((
                        k[project_prefix.len()..].to_string().to_lowercase(),
                        TomlValue::String(v),
                    ))
                })
                .collect();
            project.data.extend(vars);
            project.data.extend(global_vars.clone());
        }

        debug!("tanu configuration loaded from env: {self:#?}");
    }

    /// Get the current color theme
    pub fn color_theme(&self) -> Option<&str> {
        self.tui.payload.color_theme.as_deref()
    }
}

/// tanu's project configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectConfig {
    /// Project name specified by user.
    pub name: String,
    /// Keys and values specified by user.
    #[serde(flatten)]
    pub data: HashMap<String, TomlValue>,
    /// List of files to ignore in the project.
    #[serde(default)]
    pub test_ignore: Vec<String>,
    #[serde(default)]
    pub retry: RetryConfig,
}

impl ProjectConfig {
    pub fn get(&self, key: impl AsRef<str>) -> Result<&TomlValue> {
        let key = key.as_ref();
        self.data
            .get(key)
            .ok_or_else(|| Error::ValueNotFound(key.to_string()))
    }

    pub fn get_str(&self, key: impl AsRef<str>) -> Result<&str> {
        let key = key.as_ref();
        self.get(key)?
            .as_str()
            .ok_or_else(|| Error::ValueNotFound(key.to_string()))
    }

    pub fn get_int(&self, key: impl AsRef<str>) -> Result<i64> {
        self.get_str(key)?
            .parse()
            .map_err(|e| Error::ValueError(eyre::Error::from(e)))
    }

    pub fn get_float(&self, key: impl AsRef<str>) -> Result<f64> {
        self.get_str(key)?
            .parse()
            .map_err(|e| Error::ValueError(eyre::Error::from(e)))
    }

    pub fn get_bool(&self, key: impl AsRef<str>) -> Result<bool> {
        self.get_str(key)?
            .parse()
            .map_err(|e| Error::ValueError(eyre::Error::from(e)))
    }

    pub fn get_datetime(&self, key: impl AsRef<str>) -> Result<DateTime<Utc>> {
        self.get_str(key)?
            .parse::<DateTime<Utc>>()
            .map_err(|e| Error::ValueError(eyre::Error::from(e)))
    }

    pub fn get_array<T: DeserializeOwned>(&self, key: impl AsRef<str>) -> Result<Vec<T>> {
        serde_json::from_str(self.get_str(key)?)
            .map_err(|e| Error::ValueError(eyre::Error::from(e)))
    }

    pub fn get_object<T: DeserializeOwned>(&self, key: impl AsRef<str>) -> Result<T> {
        serde_json::from_str(self.get_str(key)?)
            .map_err(|e| Error::ValueError(eyre::Error::from(e)))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    /// Number of retries.
    #[serde(default)]
    pub count: Option<usize>,
    /// Factor to multiply the delay between retries.
    #[serde(default)]
    pub factor: Option<f32>,
    /// Whether to add jitter to the delay between retries.
    #[serde(default)]
    pub jitter: Option<bool>,
    /// Minimum delay between retries.
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub min_delay: Option<Duration>,
    /// Maximum delay between retries.
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub max_delay: Option<Duration>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            count: Some(0),
            factor: Some(2.0),
            jitter: Some(false),
            min_delay: Some(Duration::from_secs(1)),
            max_delay: Some(Duration::from_secs(60)),
        }
    }
}

impl RetryConfig {
    pub fn backoff(&self) -> backon::ExponentialBuilder {
        let builder = backon::ExponentialBuilder::new()
            .with_max_times(self.count.unwrap_or_default())
            .with_factor(self.factor.unwrap_or(2.0))
            .with_min_delay(self.min_delay.unwrap_or(Duration::from_secs(1)))
            .with_max_delay(self.max_delay.unwrap_or(Duration::from_secs(60)));

        if self.jitter.unwrap_or_default() {
            builder.with_jitter()
        } else {
            builder
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::{time::Duration, vec};
    use test_case::test_case;

    fn load_test_config() -> eyre::Result<Config> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let config_path = Path::new(manifest_dir).join("../tanu-sample.toml");
        Ok(super::Config::load_from(&config_path)?)
    }

    fn load_test_project_config() -> eyre::Result<ProjectConfig> {
        Ok(load_test_config()?.projects.remove(0))
    }

    #[test]
    fn load_config() -> eyre::Result<()> {
        let cfg = load_test_config()?;
        assert_eq!(cfg.projects.len(), 1);

        let project = &cfg.projects[0];
        assert_eq!(project.name, "default");
        assert_eq!(project.test_ignore, Vec::<String>::new());
        assert_eq!(project.retry.count, Some(0));
        assert_eq!(project.retry.factor, Some(2.0));
        assert_eq!(project.retry.jitter, Some(false));
        assert_eq!(project.retry.min_delay, Some(Duration::from_secs(1)));
        assert_eq!(project.retry.max_delay, Some(Duration::from_secs(60)));

        Ok(())
    }

    #[test_case("TANU_DEFAULT_STR_KEY"; "project config")]
    #[test_case("TANU_STR_KEY"; "global config")]
    fn get_str(key: &str) -> eyre::Result<()> {
        std::env::set_var(key, "example_string");
        let project = load_test_project_config()?;
        assert_eq!(project.get_str("str_key")?, "example_string");
        Ok(())
    }

    #[test_case("TANU_DEFAULT_INT_KEY"; "project config")]
    #[test_case("TANU_INT_KEY"; "global config")]
    fn get_int(key: &str) -> eyre::Result<()> {
        std::env::set_var(key, "42");
        let project = load_test_project_config()?;
        assert_eq!(project.get_int("int_key")?, 42);
        Ok(())
    }

    #[test_case("TANU_DEFAULT"; "project config")]
    #[test_case("TANU"; "global config")]
    fn get_float(prefix: &str) -> eyre::Result<()> {
        std::env::set_var(format!("{prefix}_FLOAT_KEY"), "5.5");
        let project = load_test_project_config()?;
        assert_eq!(project.get_float("float_key")?, 5.5);
        Ok(())
    }

    #[test_case("TANU_DEFAULT_BOOL_KEY"; "project config")]
    #[test_case("TANU_BOOL_KEY"; "global config")]
    fn get_bool(key: &str) -> eyre::Result<()> {
        std::env::set_var(key, "true");
        let project = load_test_project_config()?;
        assert_eq!(project.get_bool("bool_key")?, true);
        Ok(())
    }

    #[test_case("TANU_DEFAULT_DATETIME_KEY"; "project config")]
    #[test_case("TANU_DATETIME_KEY"; "global config")]
    fn get_datetime(key: &str) -> eyre::Result<()> {
        let datetime_str = "2025-03-08T12:00:00Z";
        std::env::set_var(key, datetime_str);
        let project = load_test_project_config()?;
        assert_eq!(
            project
                .get_datetime("datetime_key")?
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            datetime_str
        );
        Ok(())
    }

    #[test_case("TANU_DEFAULT_ARRAY_KEY"; "project config")]
    #[test_case("TANU_ARRAY_KEY"; "global config")]
    fn get_array(key: &str) -> eyre::Result<()> {
        std::env::set_var(key, "[1, 2, 3]");
        let project = load_test_project_config()?;
        let array: Vec<i64> = project.get_array("array_key")?;
        assert_eq!(array, vec![1, 2, 3]);
        Ok(())
    }

    #[test_case("TANU_DEFAULT"; "project config")]
    #[test_case("TANU"; "global config")]
    fn get_object(prefix: &str) -> eyre::Result<()> {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Foo {
            foo: Vec<String>,
        }
        std::env::set_var(
            format!("{prefix}_OBJECT_KEY"),
            "{\"foo\": [\"bar\", \"baz\"]}",
        );
        let project = load_test_project_config()?;
        let obj: Foo = project.get_object("object_key")?;
        assert_eq!(obj.foo, vec!["bar", "baz"]);
        Ok(())
    }
}
