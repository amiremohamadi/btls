use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    pub diagnostics: bool,
}

impl Config {
    pub fn from_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}
