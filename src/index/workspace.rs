use super::*;

use std::path::PathBuf;

use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    sws_path: PathBuf,
    root_folder: PathBuf,
    dataflex_version: Option<DataFlexVersion>,
    projects: Vec<ProjectInfo>,
    local_packages: Vec<PathBuf>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    main_file: PathBuf,
    toolchain: Option<String>,
    make_path: Option<Vec<PathBuf>>,
}

#[derive(Deserialize)]
struct RawWorkspaceFile {
    df: serde_json::Number,
    projects: Option<Vec<String>>,
    dependencies: Option<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct RawWorkspaceConfig {
    projects: Vec<RawWorkspaceConfigProject>,
}

#[derive(Deserialize)]
struct RawWorkspaceConfigProject {
    name: String,
    toolchain: String,
    makepath: Vec<PathBuf>,
}

impl WorkspaceInfo {
    pub fn new() -> Self {
        Self {
            sws_path: PathBuf::new(),
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
        if let Some(workspace_info) = Self::load_from_str(&content, path.clone()) {
            workspace_info
        } else {
            log::warn!("Unable to load workspace information from {:?}", path);
            Self {
                sws_path: path.clone(),
                root_folder: path.clone(),
                dataflex_version: None,
                projects: Vec::new(),
                local_packages: Vec::new(),
            }
        }
    }

    pub fn load_from_str(content: &str, path: PathBuf) -> Option<Self> {
        if let Ok(raw_workspace_file) = serde_json::from_str::<RawWorkspaceFile>(&content) {
            let root_folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let dataflex_version = Some(DataFlexVersion::from(raw_workspace_file.df.to_string()));
            let projects: Vec<ProjectInfo> = raw_workspace_file
                .projects
                .unwrap_or_default()
                .iter()
                .map(|f| ProjectInfo {
                    main_file: root_folder.join("AppSrc").join(f),
                    toolchain: None,
                    make_path: None,
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
            Some(Self {
                sws_path: path,
                root_folder,
                dataflex_version,
                projects,
                local_packages,
            })
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
                    toolchain: None,
                    make_path: None,
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
            Some(Self {
                sws_path: path,
                root_folder,
                dataflex_version,
                projects,
                local_packages,
            })
        } else {
            None
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

    pub fn fetch_package_dependencies_and_extended_info(&self) -> Option<WorkspaceInfo> {
        let df_cli = DataFlexConfig::system_config().df_cli_path()?;
        log::info!("Using df_cli from: {:?}", df_cli);

        let output = std::process::Command::new(&df_cli)
            .arg("config")
            .arg("--json")
            .arg(&self.sws_path)
            .output();
        log::trace!("df_cli output: {:?}", output);
        let output = output.ok()?;

        if output.status.success() {
            let config = serde_json::from_slice::<RawWorkspaceConfig>(&output.stdout).ok()?;
            let mut workspace = self.clone();
            // Merge in additional project info with computed makepath etc., and return the extended WorkspaceInfo.
            for project in config.projects {
                if let Some(existing_project) = workspace
                    .projects
                    .iter_mut()
                    .find(|p| p.main_file.ends_with(&project.name))
                {
                    existing_project.toolchain.replace(project.toolchain);
                    existing_project.make_path.replace(project.makepath);
                }
            }
            Some(workspace)
        } else {
            // Try running df-cli without --json output since older versions don't support it, just to fetch packages.
            _ = std::process::Command::new(df_cli)
                .arg("config")
                .arg(&self.sws_path)
                .output();
            None
        }
    }

    fn find_first_sws(path: &PathBuf) -> Option<PathBuf> {
        path.read_dir().ok()?.find_map(|f| {
            let file_path = f.ok()?.path();
            if file_path.extension()?.to_str()?.eq_ignore_ascii_case("sws") {
                Some(file_path)
            } else {
                None
            }
        })
    }
}
