pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Occurs when `tanu.toml` fails to load.
    #[error("failed to load tanu.toml: {0}")]
    LoadError(String),
    /// Occurs when the specified key is not found in `tanu.toml`.
    #[error("the specified key \"{0}\" not found in tanu.toml")]
    ValueNotFound(String),
}
