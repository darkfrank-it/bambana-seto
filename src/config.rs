use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub log_path: String,
    pub database_path: String,
    pub locale: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_path: ".data/bambana.log".to_string(),
            database_path: ".data/bambana.db".to_string(),
            locale: "en".to_string(),
        }
    }
}

impl Config {
    pub fn database_url(&self) -> String {
        format!("sqlite:{}", self.database_path)
    }
}
