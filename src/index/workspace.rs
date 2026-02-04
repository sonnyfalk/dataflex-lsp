use std::path::PathBuf;

use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug)]
pub struct WorkspaceInfo {
    root_folder: PathBuf,
    dataflex_version: Option<DataFlexVersion>,
    projects: Vec<ProjectInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DataFlexVersion(String);

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

#[allow(dead_code)]
#[derive(Debug)]
pub struct ProjectInfo {
    main_file: PathBuf,
}

#[derive(Deserialize)]
struct RawWorkspaceFile {
    df: serde_json::Number,
    projects: Option<Vec<String>>,
}

impl WorkspaceInfo {
    pub fn new() -> Self {
        Self {
            root_folder: PathBuf::new(),
            dataflex_version: None,
            projects: vec![],
        }
    }

    pub fn load_from_path(path: &PathBuf) -> Self {
        if path.is_dir() {
            if let Some(file) = Self::find_first_sws(path) {
                return Self::load_from_path(&file);
            }
        }

        let content = std::fs::read_to_string(path).unwrap_or_default();

        if let Some(raw_workspace_file) = serde_json::from_str::<RawWorkspaceFile>(&content).ok() {
            let root_folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let dataflex_version = Some(DataFlexVersion::from(raw_workspace_file.df.to_string()));
            let projects: Vec<ProjectInfo> = raw_workspace_file
                .projects
                .unwrap_or_default()
                .iter()
                .map(|f| ProjectInfo {
                    main_file: root_folder.join("AppSrc").join(f),
                })
                .collect();
            Self {
                root_folder,
                dataflex_version,
                projects,
            }
        } else if let Some(ini_file) = ini::Ini::load_from_str(&content).ok() {
            let root_folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let dataflex_version = ini_file
                .section(Some("Properties"))
                .and_then(|properties| properties.get("Version"))
                .map(DataFlexVersion::from);
            let projects: Vec<ProjectInfo> = ini_file
                .section(Some("Projects"))
                .map(|projects| {
                    projects
                        .iter()
                        .map(|(_, v)| ProjectInfo {
                            main_file: root_folder.join("AppSrc").join(v),
                        })
                        .collect()
                })
                .unwrap_or_default();
            Self {
                root_folder,
                dataflex_version,
                projects,
            }
        } else {
            log::warn!("Unable to load workspace information from {:?}", path);
            Self {
                root_folder: path.clone(),
                dataflex_version: None,
                projects: vec![],
            }
        }
    }

    pub fn get_root_folder(&self) -> &PathBuf {
        &self.root_folder
    }

    pub fn get_dataflex_version(&self) -> Option<&DataFlexVersion> {
        self.dataflex_version.as_ref()
    }

    fn find_first_sws(path: &PathBuf) -> Option<PathBuf> {
        path.read_dir().ok()?.find_map(|f| {
            let file_path = f.ok()?.path();
            if file_path.extension()?.to_str() == Some("sws") {
                Some(file_path)
            } else {
                None
            }
        })
    }
}
