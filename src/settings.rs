use log::LevelFilter;
use serde::{Deserialize, Deserializer};
use std::str::FromStr;

use crate::logging;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(deserialize_with = "Settings::deserialize_log_level")]
    pub log_level: LevelFilter,
    pub fetch_package_dependencies: FetchPackageDependencies,
}

#[derive(Debug, Deserialize, Default)]
pub enum FetchPackageDependencies {
    #[default]
    Auto,
    Always,
    Never,
}

static SETTINGS: std::sync::RwLock<Settings> = std::sync::RwLock::new(Settings::new());

impl Settings {
    pub const fn new() -> Self {
        Self {
            log_level: LevelFilter::Info,
            fetch_package_dependencies: FetchPackageDependencies::Auto,
        }
    }

    pub fn get() -> impl std::ops::Deref<Target = Self> {
        SETTINGS.read().unwrap()
    }

    pub fn set(settings: Settings) {
        log::info!("settings = {:?}", settings);
        *SETTINGS.write().unwrap() = settings;
        logging::set_log_level_filter(Self::get().log_level);
    }

    fn deserialize_log_level<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<LevelFilter, D::Error> {
        let s = String::deserialize(deserializer)?;
        LevelFilter::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl std::default::Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
