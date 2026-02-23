use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub struct Config {
    pub processes: Vec<ProcessConfig>,
}

#[derive(Deserialize)]
pub struct ProcessConfig {
    pub name: String,
    pub cmd: Vec<String>,
    pub cwd: Option<String>
}

pub fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}
