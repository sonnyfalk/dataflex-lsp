use super::*;

#[derive(Debug)]
pub struct DataFlexConfig {
    versioned_system_paths: HashMap<DataFlexVersion, Vec<PathBuf>>,
    default_version: DataFlexVersion,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DataFlexVersion(String);

static SYSTEM_DATAFLEX_CONFIG: std::sync::LazyLock<DataFlexConfig> =
    std::sync::LazyLock::new(|| DataFlexConfig::new());

impl DataFlexConfig {
    pub fn system_config() -> &'static Self {
        &SYSTEM_DATAFLEX_CONFIG
    }

    pub fn system_path(&self, dataflex_version: Option<&DataFlexVersion>) -> Option<&Vec<PathBuf>> {
        let dataflex_version = dataflex_version.unwrap_or(&self.default_version);
        self.versioned_system_paths
            .get(dataflex_version)
            .or(self.versioned_system_paths.get(&self.default_version))
    }

    fn new() -> Self {
        if let Some(versioned_system_paths) = Self::versioned_system_paths() {
            let default_version = versioned_system_paths
                .keys()
                .next()
                .cloned()
                .unwrap_or_default();
            Self {
                versioned_system_paths,
                default_version,
            }
        } else {
            Self {
                versioned_system_paths: HashMap::new(),
                default_version: Default::default(),
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn versioned_system_paths() -> Option<HashMap<DataFlexVersion, Vec<PathBuf>>> {
        let reg_key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Data Access Worldwide\\DataFlex")
            .ok()?;

        Some(reg_key.enum_keys().flat_map(Result::ok).fold(
            HashMap::new(),
            |mut result, version: String| {
                let make_path: Option<String> = reg_key
                    .open_subkey(format!("{version}\\DfComp"))
                    .and_then(|k| k.get_value("MakePath"))
                    .ok();
                if let Some(make_path) = make_path {
                    result.insert(
                        DataFlexVersion::from(version),
                        make_path
                            .split(";")
                            .map(str::trim)
                            .map(PathBuf::from)
                            .collect(),
                    );
                }
                result
            },
        ))
    }

    #[cfg(not(target_os = "windows"))]
    fn versioned_system_paths() -> Option<HashMap<DataFlexVersion, Vec<PathBuf>>> {
        None
    }
}

impl From<String> for DataFlexVersion {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for DataFlexVersion {
    fn from(value: &str) -> Self {
        Self::from(String::from(value))
    }
}

impl Default for DataFlexVersion {
    fn default() -> Self {
        Self::from(String::new())
    }
}
