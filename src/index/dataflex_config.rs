use super::*;

#[derive(Debug)]
pub struct DataFlexConfig {
    versioned_configs: HashMap<DataFlexVersion, ConfigEntry>,
    default_version: DataFlexVersion,
}

#[derive(Debug)]
struct ConfigEntry {
    root_path: PathBuf,
    system_make_path: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DataFlexVersion {
    pub major: usize,
    pub minor: usize,
}

static SYSTEM_DATAFLEX_CONFIG: std::sync::LazyLock<DataFlexConfig> =
    std::sync::LazyLock::new(|| DataFlexConfig::new());

impl DataFlexConfig {
    pub fn system_config() -> &'static Self {
        &SYSTEM_DATAFLEX_CONFIG
    }

    pub fn system_path(&self, dataflex_version: Option<&DataFlexVersion>) -> Option<&Vec<PathBuf>> {
        let dataflex_version = dataflex_version.unwrap_or(&self.default_version);
        self.versioned_configs
            .get(dataflex_version)
            .or(self.versioned_configs.get(&self.default_version))
            .map(|config| &config.system_make_path)
    }

    pub fn df_cli_path(&self) -> Option<PathBuf> {
        if let Ok(df_cli) = which::which_global("df-cli") {
            return Some(df_cli);
        }

        let mut installs: Vec<_> = self.versioned_configs.iter().collect();
        installs.sort_by(|a, b| a.0.cmp(b.0).reverse());
        let paths = std::env::join_paths(
            installs
                .into_iter()
                .map(|(_, config)| config.root_path.join("Bin64")),
        )
        .ok();

        which::which_in_global("df-cli", paths).ok()?.next()
    }

    fn new() -> Self {
        if let Some(versioned_configs) = Self::versioned_configs() {
            let default_version = versioned_configs.keys().max().cloned().unwrap_or_default();
            Self {
                versioned_configs,
                default_version,
            }
        } else {
            Self {
                versioned_configs: HashMap::new(),
                default_version: Default::default(),
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn versioned_configs() -> Option<HashMap<DataFlexVersion, ConfigEntry>> {
        let reg_key = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE)
            .open_subkey("SOFTWARE\\Data Access Worldwide\\DataFlex")
            .ok()?;

        Some(reg_key.enum_keys().flat_map(Result::ok).fold(
            HashMap::new(),
            |mut result, version: String| {
                let root_path: Option<String> = reg_key
                    .open_subkey(format!("{version}\\Defaults"))
                    .and_then(|k| k.get_value("VDFRootDir"))
                    .ok();
                let make_path: Option<String> = reg_key
                    .open_subkey(format!("{version}\\Workspaces"))
                    .and_then(|k| k.get_value("SystemMakePath"))
                    .ok();
                if let Some(root_path) = root_path
                    && let Some(make_path) = make_path
                {
                    result.insert(
                        DataFlexVersion::from(version),
                        ConfigEntry {
                            root_path: root_path.into(),
                            system_make_path: make_path
                                .split(";")
                                .map(str::trim)
                                .map(PathBuf::from)
                                .collect(),
                        },
                    );
                }
                result
            },
        ))
    }

    #[cfg(not(target_os = "windows"))]
    fn versioned_configs() -> Option<HashMap<DataFlexVersion, ConfigEntry>> {
        None
    }
}

impl DataFlexVersion {
    pub fn new(major: usize, minor: usize) -> Self {
        Self { major, minor }
    }

    pub fn dataflex_26() -> Self {
        Self::new(26, 0)
    }
}

impl From<String> for DataFlexVersion {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&str> for DataFlexVersion {
    fn from(value: &str) -> Self {
        let mut version_numbers = value
            .split('.')
            .filter_map(|part| part.parse::<usize>().ok());

        Self {
            major: version_numbers.next().unwrap_or_default(),
            minor: version_numbers.next().unwrap_or_default(),
        }
    }
}

impl Default for DataFlexVersion {
    fn default() -> Self {
        Self::dataflex_26()
    }
}
