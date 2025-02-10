use std::path::PathBuf;

#[allow(dead_code)]
#[derive(Debug)]
pub struct WorkspaceInfo {
    root_folder: PathBuf,
    dataflex_version: Option<String>,
    projects: Vec<ProjectInfo>,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct ProjectInfo {
    main_file: PathBuf,
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

        if let Some(ini_file) = ini::Ini::load_from_file(path).ok() {
            let root_folder = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            let dataflex_version = ini_file.section(Some("Properties")).and_then(|properties| properties.get("Version")).map(String::from);
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

    pub fn get_dataflex_version(&self) -> Option<&String> {
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
