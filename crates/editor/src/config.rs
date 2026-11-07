use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub font_size: f32,
    pub theme: Theme,
    pub autosave_enabled: bool,
    pub autosave_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            theme: Theme::Dark,
            autosave_enabled: true,
            autosave_interval_secs: 30,
        }
    }
}

impl AppConfig {
    pub fn autosave_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.autosave_interval_secs.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_toml() {
        let cfg = AppConfig {
            font_size: 18.0,
            theme: Theme::Light,
            autosave_interval_secs: 10,
            ..Default::default()
        };
        let s = toml::to_string_pretty(&cfg).unwrap();
        let back: AppConfig = toml::from_str(&s).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn missing_fields_default() {
        let cfg: AppConfig = toml::from_str("").unwrap();
        assert_eq!(cfg, AppConfig::default());
    }
}
