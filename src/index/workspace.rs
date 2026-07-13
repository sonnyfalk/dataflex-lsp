use std::path::PathBuf;

use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug)]
pub struct WorkspaceInfo {
    root_folder: PathBuf,
    dataflex_version: Option<DataFlexVersion>,
    projects: Vec<ProjectInfo>,
    local_packages: Vec<PathBuf>,
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
    dependencies: Option<Vec<serde_json::Value>>,
}

impl WorkspaceInfo {
    pub fn new() -> Self {
        Self {
            root_folder: PathBuf::new(),
            dataflex_version: None,
            projects: Vec::new(),
            local_packages: Vec::new(),
        }
    }

    pub fn load_from_path(path: &PathBuf) -> Self {
        if path.is_dir()
            && let Some(file) = Self::find_first_sws(path)
        {
            return Self::load_from_path(&file);
        }

        let content = std::fs::read_to_string(path).unwrap_or_default();

        if let Ok(raw_workspace_file) = serde_json::from_str::<RawWorkspaceFile>(&content) {
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
            let local_packages: Vec<PathBuf> = raw_workspace_file
                .dependencies
                .iter()
                .flat_map(|d| d.iter())
                .filter_map(|dependency| {
                    if let serde_json::Value::String(s) = dependency {
                        Some(s)
                    } else {
                        None
                    }
                })
                .filter(|s| s.starts_with("..") || s.starts_with("/"))
                .map(PathBuf::from)
                .filter_map(|p| {
                    if p.is_relative() {
                        std::path::absolute(root_folder.join(&p)).ok()
                    } else {
                        Some(p)
                    }
                })
                .collect();
            Self {
                root_folder,
                dataflex_version,
                projects,
                local_packages,
            }
        } else if let Ok(ini_file) = ini::Ini::load_from_str_opt(
            &content,
            ini::ParseOption {
                enabled_escape: false,
                ..Default::default()
            },
        ) {
            let root_folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let dataflex_version = ini_file
                .section(Some("Properties"))
                .and_then(|properties| properties.get("Version"))
                .map(DataFlexVersion::from);
            let projects: Vec<ProjectInfo> = ini_file
                .section(Some("Projects"))
                .iter()
                .flat_map(|projects| projects.iter())
                .map(|(_, v)| ProjectInfo {
                    main_file: root_folder.join("AppSrc").join(v),
                })
                .collect();
            let local_packages: Vec<PathBuf> = ini_file
                .section(Some("Libraries"))
                .iter()
                .flat_map(|libraries| libraries.iter())
                .map(|(_, l)| PathBuf::from(l))
                .filter_map(|p| {
                    if p.is_relative() && p.starts_with("..") {
                        std::path::absolute(root_folder.join(&p)).ok()
                    } else if p.is_absolute() {
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect();
            Self {
                root_folder,
                dataflex_version,
                projects,
                local_packages,
            }
        } else {
            log::warn!("Unable to load workspace information from {:?}", path);
            Self {
                root_folder: path.clone(),
                dataflex_version: None,
                projects: Vec::new(),
                local_packages: Vec::new(),
            }
        }
    }

    pub fn get_root_folder(&self) -> &PathBuf {
        &self.root_folder
    }

    pub fn get_dataflex_version(&self) -> Option<&DataFlexVersion> {
        self.dataflex_version.as_ref()
    }

    pub fn local_workspace_dependencies(&self) -> Vec<WorkspaceInfo> {
        let mut workspaces = Vec::new();
        let mut dependencies = self.local_packages.clone();
        let mut visited = std::collections::HashSet::new();

        while let Some(dependency) = dependencies.pop() {
            if visited.insert(dependency.clone()) {
                let workspace = WorkspaceInfo::load_from_path(&dependency);
                dependencies.extend(workspace.local_packages.iter().cloned());
                workspaces.push(workspace);
            }
        }

        workspaces
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
