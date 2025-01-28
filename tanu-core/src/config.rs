use once_cell::sync::Lazy;
use serde::Deserialize;
use std::{collections::HashMap, io::Read};
use toml::Value as TomlValue;
use tracing::*;

use crate::{Error, Result};

static CONFIG: Lazy<Config> = Lazy::new(|| Config::load().expect("failed to load tanu.toml"));

tokio::task_local! {
    pub static PROJECT: ProjectConfig;
}

pub fn get_tanu_config() -> &'static Config {
    &CONFIG
}

/// Get configuration for the current project. This function has to be called in the tokio
/// task created by tanu runner. Otherwise, calling this function will panic.
pub fn get_config() -> ProjectConfig {
    PROJECT.get()
}

/// tanu's configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    pub projects: Vec<ProjectConfig>,
}

impl Config {
    /// Load tanu configuration from tanu.toml
    fn load() -> Result<Config> {
        let Ok(mut file) = std::fs::File::open("tanu.toml") else {
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
        static PREFIX: &'static str = "TANU";

        let global_prefix = format!("{PREFIX}_");
        let project_prefixes: Vec<_> = self
            .projects
            .iter()
            .map(|p| format!("{PREFIX}_PROJECT_{}_", p.name.to_uppercase()))
            .collect();
        let global_vars: HashMap<_, _> = std::env::vars()
            .filter_map(|(k, v)| {
                let is_project_var = project_prefixes.iter().any(|pp| k.find(pp).is_some());
                if is_project_var {
                    return None;
                }

                k.find(&global_prefix)?;
                if global_prefix.len() >= k.len() {
                    None
                } else {
                    debug!("Loading {k} from env");
                    Some((
                        k[global_prefix.len()..].to_string().to_lowercase(),
                        TomlValue::String(v),
                    ))
                }
            })
            .collect();

        for project in &mut self.projects {
            let project_prefix = format!("{PREFIX}_PROJECT_{}_", project.name.to_uppercase());
            let vars: HashMap<_, _> = std::env::vars()
                .filter_map(|(k, v)| {
                    k.find(&project_prefix)?;
                    if project_prefix.len() >= k.len() {
                        None
                    } else {
                        debug!("Loading {k} from env");
                        Some((
                            k[project_prefix.len()..].to_string().to_lowercase(),
                            TomlValue::String(v),
                        ))
                    }
                })
                .collect();
            project.data.extend(vars);
            project.data.extend(global_vars.clone());
        }

        debug!("tanu configuration loaded from env: {self:#?}");
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
    #[serde(default)]
    pub test_ignore: Vec<String>,
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
}
