use serde::{Deserialize, Serialize};

fn default_pixels_per_point() -> f32 {
    1.2
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub log_path: String,
    pub database_path: String,
    pub locale: String,
    #[serde(default = "default_pixels_per_point")]
    pub pixels_per_point: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_path: ".data/bambana.log".to_string(),
            database_path: ".data/bambana.db".to_string(),
            locale: "en".to_string(),
            pixels_per_point: 1.2,
        }
    }
}

impl Config {
    pub fn database_url(&self) -> String {
        format!("sqlite:{}", self.database_path)
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn deserializes_legacy_config_without_pixels_per_point() {
        let input = r#"
log_path = ".data/bambana.log"
database_path = ".data/bambana.db"
locale = "it"
"#;

        let config: Config = toml::from_str(input).unwrap();

        assert_eq!(config.locale, "it");
        assert_eq!(config.log_path, ".data/bambana.log");
        assert_eq!(config.database_path, ".data/bambana.db");
        assert!((config.pixels_per_point - 1.2).abs() < f32::EPSILON);
    }
}
