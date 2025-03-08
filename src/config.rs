//! Configuration related structures
use anyhow::Result;
use serde::Deserialize;
use std::{collections::HashMap, fs::read_to_string};

#[derive(Debug, Clone, Deserialize)]
/// The global configuration
pub struct Config {
    pub connections: HashMap<String, ConnectionConfig>,
}

impl Config {
    /// Creates a new `Config` instance using the parameters found in the given
    /// toml configuration file. If the file could not be found or the file is
    /// invalid, an `Error` will be returned.
    pub fn from_file(filename: &str) -> Result<Self> {
        Ok(toml::from_str(&read_to_string(filename)?)?)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// A connection configuration
pub struct ConnectionConfig {
    pub name: String,
    pub context: String,
    pub up_script: String,
    pub down_script: String,
}
